/**
 * @module primitives/io
 *
 * Promise facade for metatool runtime I/O primitives.
 *
 * Runtime payloads are Schema-defined and execution is owned by the
 * MetatoolRuntime Effect service. This file only preserves the existing
 * `cm.read` / `cm.write` / `cm.sh` Promise API for eval callers.
 */

import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import * as ManagedRuntime from "effect/ManagedRuntime"
import { FileSystem } from "effect/FileSystem"
import { MetatoolRuntime, makeNodeRuntimeLayer } from "../services/MetatoolRuntime.js"
import type { RuntimeCommand, RuntimeExecOptions, RuntimeExecResult } from "../schemas/runtime.js"

// ── Types ────────────────────────────────────────────────────────

export interface IoApi {
  /** Read a file (cwd-relative or absolute). Returns content as string. */
  read(path: string): Promise<string>

  /** Write a file (cwd-relative or absolute, auto-creates parent dirs). */
  write(path: string, content: string): Promise<void>

  /**
   * Execute a shell command (cwd-scoped, 15s timeout).
   * Returns stdout on success, stdout+stderr on failure.
   */
  sh(cmd: string): Promise<string>

  /** Execute through the configured runtime boundary. */
  exec(command: RuntimeCommand, options?: RuntimeExecOptions): Promise<RuntimeExecResult>

  /** Dispose the ManagedRuntime */
  dispose(): Promise<void>
}

// ── Factory ──────────────────────────────────────────────────────

/**
 * Create IO primitives backed by a MetatoolRuntime service.
 *
 * @param cwd - Working directory for relative paths and shell commands
 * @param fsLayer - Effect Layer providing FileSystem for the default Node runtime
 * @param runtimeLayer - Optional execution runtime; defaults to Node compatibility
 */
export function createIoApi(
  cwd: string,
  fsLayer: Layer.Layer<FileSystem>,
  runtimeLayer: Layer.Layer<MetatoolRuntime, any, FileSystem> = makeNodeRuntimeLayer(cwd),
): IoApi {
  const appLayer = runtimeLayer.pipe(Layer.provide(fsLayer))
  const runtime = ManagedRuntime.make(appLayer)
  const run = <A>(effect: Effect.Effect<A, any, any>): Promise<A> =>
    runtime.runPromise(effect)

  // ── read ─────────────────────────────────────────────────────

  const read = (path: string): Promise<string> =>
    run(MetatoolRuntime.pipe(
      Effect.flatMap((rt) => rt.read(path)),
    ))

  // ── write ────────────────────────────────────────────────────

  const write = (path: string, content: string): Promise<void> =>
    run(MetatoolRuntime.pipe(
      Effect.flatMap((rt) => rt.write(path, content)),
    ))

  // ── sh ───────────────────────────────────────────────────────
  //
  // Compatibility wrapper. New runtime-aware code should call exec through the
  // MetatoolRuntime service rather than depend on shell-string output.

  const sh = (cmd: string): Promise<string> =>
    run(MetatoolRuntime.pipe(
      Effect.flatMap((rt) => rt.exec({ shell: cmd })),
      Effect.map((result) => `${result.stdout ?? ""}${result.stderr ?? ""}`.trim()),
      Effect.catch((e) => Effect.succeed(e.message)),
    ))

  const execRuntime = (command: RuntimeCommand, options?: RuntimeExecOptions): Promise<RuntimeExecResult> =>
    run(MetatoolRuntime.pipe(
      Effect.flatMap((rt) => rt.exec(command, options)),
    ))

  // ── dispose ──────────────────────────────────────────────────

  const dispose = (): Promise<void> => runtime.dispose()

  return { read, write, sh, exec: execRuntime, dispose }
}
