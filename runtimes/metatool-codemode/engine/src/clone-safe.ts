/**
 * Clone-safe payload sanitizer for the metatool `mt` tool.
 *
 * Pi stores tool results in session context and clones that context with
 * structuredClone() before subsequent model calls. Arbitrary eval output can
 * contain Promises, functions, proxies, cycles, class instances, or values that
 * JSON renders poorly. This module resolves/summarizes those values at the tool
 * boundary so one bad REPL result cannot poison the whole session.
 *
 * @module
 */

export interface SanitizationOptions {
  /** Maximum recursion depth before a subtree is summarized. */
  maxDepth?: number
  /** Maximum array/set/map items to retain. */
  maxArrayItems?: number
  /** Maximum enumerable object keys to retain. */
  maxObjectKeys?: number
  /** Timeout for nested unawaited Promises. */
  promiseTimeoutMs?: number
  /** Tool cancellation signal. */
  signal?: AbortSignal
}

export interface SanitizationResult {
  value: unknown
  warnings: string[]
}

const DEFAULT_OPTIONS: Required<Omit<SanitizationOptions, 'signal'>> = {
  maxDepth: 10,
  maxArrayItems: 500,
  maxObjectKeys: 500,
  promiseTimeoutMs: 30_000,
}

const TIMEOUT = Symbol('timeout')
const ABORTED = Symbol('aborted')

type SanitizerState = {
  options: Required<Omit<SanitizationOptions, 'signal'>> & { signal?: AbortSignal }
  warnings: string[]
  seen: WeakMap<object, string>
}

/**
 * Resolve nested Promises and replace non-cloneable / non-JSON-safe values with
 * explicit summaries. The returned value is intended to be safe for both
 * JSON.stringify() and structuredClone().
 */
export async function sanitizeForToolPayload(
  value: unknown,
  options: SanitizationOptions = {},
): Promise<SanitizationResult> {
  const state: SanitizerState = {
    options: { ...DEFAULT_OPTIONS, ...options },
    warnings: [],
    seen: new WeakMap(),
  }

  const sanitized = await visit(value, '$', 0, state)
  return { value: sanitized, warnings: state.warnings }
}

/** Format sanitized data for the LLM-facing content text. */
export function stringifyForToolContent(value: unknown): string {
  if (value === undefined) return '(void — side effect only)'
  if (typeof value === 'string') return value

  try {
    const json = JSON.stringify(value, null, 2)
    return json === undefined ? String(value) : json
  } catch (err) {
    return JSON.stringify({
      _tag: 'StringifyError',
      message: describeError(err),
      fallback: String(value),
    }, null, 2)
  }
}

/**
 * Last-line defense for tool result `details`. If a future code path smuggles a
 * non-cloneable value past the sanitizer, degrade instead of poisoning session
 * context.
 */
export function ensureCloneSafeDetails<T extends Record<string, unknown>>(details: T): T {
  try {
    if (typeof structuredClone === 'function') structuredClone(details)
    return details
  } catch (err) {
    return {
      code: typeof details.code === 'string' ? details.code : '',
      error: `Internal metatool sanitizer fallback: ${describeError(err)}`,
      result: '[details omitted: not structured-clone safe]',
    } as unknown as T
  }
}

async function visit(value: unknown, path: string, depth: number, state: SanitizerState): Promise<unknown> {
  if (state.options.signal?.aborted) {
    state.warnings.push(`Sanitization aborted at ${path}`)
    return { _tag: 'SanitizationAborted', path }
  }

  const then = getThen(value)
  if (then) {
    return resolveThenable(value as PromiseLike<unknown>, path, depth, state)
  }

  const primitive = sanitizePrimitive(value)
  if (primitive.handled) return primitive.value

  if (depth >= state.options.maxDepth) {
    state.warnings.push(`Truncated ${path}: max depth ${state.options.maxDepth} reached`)
    return `[MaxDepth ${state.options.maxDepth} at ${path}]`
  }

  if (typeof value !== 'object' || value === null) return value

  const obj = value as object
  const previousPath = state.seen.get(obj)
  if (previousPath) {
    state.warnings.push(`Replaced circular reference at ${path} (seen at ${previousPath})`)
    return `[Circular ${previousPath}]`
  }
  state.seen.set(obj, path)

  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? '[Invalid Date]' : value.toISOString()
  }

  if (value instanceof RegExp) return String(value)

  if (typeof URL !== 'undefined' && value instanceof URL) return value.toString()

  if (value instanceof Error) {
    return sanitizeError(value)
  }

  if (value instanceof WeakMap) return '[WeakMap]'
  if (value instanceof WeakSet) return '[WeakSet]'

  if (value instanceof Map) {
    return sanitizeMap(value, path, depth, state)
  }

  if (value instanceof Set) {
    return sanitizeArray(Array.from(value), path, depth, state, 'Set')
  }

  if (Array.isArray(value)) {
    return sanitizeArray(value, path, depth, state, 'Array')
  }

  if (value instanceof ArrayBuffer) {
    return { _tag: 'ArrayBuffer', byteLength: value.byteLength }
  }

  if (ArrayBuffer.isView(value as any)) {
    const view = value as { byteLength?: number; length?: number; constructor?: { name?: string } }
    return {
      _tag: view.constructor?.name ?? 'TypedArray',
      byteLength: view.byteLength ?? 0,
      length: view.length ?? undefined,
    }
  }

  return sanitizeObject(value as Record<string, unknown>, path, depth, state)
}

function sanitizePrimitive(value: unknown): { handled: true; value: unknown } | { handled: false } {
  switch (typeof value) {
    case 'string':
    case 'boolean':
    case 'undefined':
      return { handled: true, value }
    case 'number':
      return { handled: true, value: Number.isFinite(value) ? value : { _tag: 'NonFiniteNumber', value: String(value) } }
    case 'bigint':
      return { handled: true, value: `${value.toString()}n` }
    case 'symbol':
      return { handled: true, value: `[${String(value)}]` }
    case 'function':
      return { handled: true, value: `[Function ${(value as Function).name || 'anonymous'}]` }
    case 'object':
      return value === null ? { handled: true, value: null } : { handled: false }
  }
}

function getThen(value: unknown): Function | undefined {
  if ((typeof value !== 'object' && typeof value !== 'function') || value === null) return undefined
  try {
    const then = (value as { then?: unknown }).then
    return typeof then === 'function' ? then : undefined
  } catch {
    return undefined
  }
}

async function resolveThenable(
  value: PromiseLike<unknown>,
  path: string,
  depth: number,
  state: SanitizerState,
): Promise<unknown> {
  state.warnings.push(`Resolved unawaited Promise at ${path}; prefer await/Promise.all before returning from mt code`)

  let timer: ReturnType<typeof setTimeout> | undefined
  let abortHandler: (() => void) | undefined

  const timeout = new Promise<typeof TIMEOUT>((resolve) => {
    timer = setTimeout(() => resolve(TIMEOUT), state.options.promiseTimeoutMs)
  })

  const abort = state.options.signal
    ? new Promise<typeof ABORTED>((resolve) => {
      abortHandler = () => resolve(ABORTED)
      state.options.signal?.addEventListener('abort', abortHandler, { once: true })
    })
    : undefined

  try {
    const raced = await Promise.race([
      Promise.resolve(value),
      timeout,
      ...(abort ? [abort] : []),
    ])

    if (raced === TIMEOUT) {
      state.warnings.push(`Promise at ${path} did not settle within ${state.options.promiseTimeoutMs}ms`)
      return { _tag: 'UnresolvedPromise', path, timeoutMs: state.options.promiseTimeoutMs }
    }

    if (raced === ABORTED) {
      state.warnings.push(`Promise at ${path} aborted by tool cancellation`)
      return { _tag: 'AbortedPromise', path }
    }

    return visit(raced, path, depth, state)
  } catch (err) {
    state.warnings.push(`Promise at ${path} rejected: ${describeError(err)}`)
    return { _tag: 'RejectedPromise', path, message: describeError(err) }
  } finally {
    if (timer) clearTimeout(timer)
    if (abortHandler) state.options.signal?.removeEventListener('abort', abortHandler)
  }
}

async function sanitizeArray(
  value: unknown[],
  path: string,
  depth: number,
  state: SanitizerState,
  tag: 'Array' | 'Set',
): Promise<unknown[]> {
  const max = state.options.maxArrayItems
  const slice = value.slice(0, max)
  const out = await Promise.all(
    slice.map((item, i) => visit(item, `${path}[${i}]`, depth + 1, state)),
  )

  if (value.length > max) {
    state.warnings.push(`Truncated ${tag} at ${path}: ${value.length - max} item(s) omitted`)
    out.push(`[${tag} truncated: ${value.length - max} more item(s)]`)
  }

  return out
}

async function sanitizeMap(
  value: Map<unknown, unknown>, path: string, depth: number, state: SanitizerState): Promise<unknown> {
  const max = state.options.maxArrayItems
  const rawEntries = Array.from(value.entries()).slice(0, max)
  const entries: unknown[] = await Promise.all(
    rawEntries.map(async ([key, val], i) => [
      await visit(key, `${path}.<key:${i}>`, depth + 1, state),
      await visit(val, `${path}.<value:${i}>`, depth + 1, state),
    ]),
  )

  if (value.size > max) {
    state.warnings.push(`Truncated Map at ${path}: ${value.size - max} entries omitted`)
    entries.push(`[Map truncated: ${value.size - max} more entries]`)
  }

  return { _tag: 'Map', entries }
}

async function sanitizeObject(
  value: Record<string, unknown>,
  path: string,
  depth: number,
  state: SanitizerState,
): Promise<Record<string, unknown>> {
  const out: Record<string, unknown> = {}
  const proto = Object.getPrototypeOf(value)
  const className = proto && proto !== Object.prototype && proto.constructor?.name

  if (className && className !== 'Object') {
    out._class = className
  }

  let keys: string[]
  try {
    keys = Object.keys(value)
  } catch (err) {
    return { _tag: 'UnreadableObject', message: describeError(err) }
  }

  const max = state.options.maxObjectKeys
  const entries = await Promise.all(
    keys.slice(0, max).map(async (key) => {
      const childPath = `${path}.${formatKey(key)}`
      try {
        return [key, await visit(value[key], childPath, depth + 1, state)] as const
      } catch (err) {
        state.warnings.push(`Could not read ${childPath}: ${describeError(err)}`)
        return [key, { _tag: 'UnreadableProperty', message: describeError(err) }] as const
      }
    }),
  )

  for (const [key, entryValue] of entries) out[key] = entryValue

  if (keys.length > max) {
    state.warnings.push(`Truncated object at ${path}: ${keys.length - max} key(s) omitted`)
    out.__truncated__ = `${keys.length - max} more key(s)`
  }

  const symbolKeys = Object.getOwnPropertySymbols(value)
  if (symbolKeys.length > 0) {
    state.warnings.push(`Dropped ${symbolKeys.length} symbol key(s) at ${path}`)
  }

  return out
}

function sanitizeError(error: Error): Record<string, unknown> {
  const out: Record<string, unknown> = {
    _tag: 'Error',
    name: error.name,
    message: error.message,
  }
  if (error.stack) out.stack = error.stack
  if ('cause' in error) out.cause = String((error as { cause?: unknown }).cause)
  return out
}

function formatKey(key: string): string {
  return /^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key)
}

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}
