/**
 * @module services/MetatoolRuntime
 *
 * Effect v4 runtime boundary for metatool/metatool execution and filesystem access.
 * Runtime payloads are Schema-defined in ../schemas/runtime.ts; this file only
 * defines the service algebra and the compatibility Node implementation.
 */

import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import { FileSystem } from "effect/FileSystem"
import { dirname, join } from "node:path"
import { exec } from "node:child_process"
import {
  validateRuntimeCommand,
  validateRuntimeExecOptions,
  validateRuntimeExecResult,
  type RuntimeCommand,
  type RuntimeError,
  type RuntimeExecOptions,
  type RuntimeExecResult,
  type RuntimeSpawnOptions,
} from "../schemas/runtime.js"

// ── Service Shape ─────────────────────────────────────────────────

export interface RuntimeProcess {
  readonly id: string
  readonly command: string
  readonly kill: () => Effect.Effect<void, RuntimeError>
}

export interface MetatoolRuntimeShape {
  readonly read: (path: string) => Effect.Effect<string, RuntimeError>
  readonly write: (path: string, content: string) => Effect.Effect<void, RuntimeError>
  readonly exec: (command: RuntimeCommand, options?: RuntimeExecOptions) => Effect.Effect<RuntimeExecResult, RuntimeError>
  readonly spawn: (command: RuntimeCommand, options?: RuntimeSpawnOptions) => Effect.Effect<RuntimeProcess, RuntimeError>
}

// ── Service ───────────────────────────────────────────────────────

export class MetatoolRuntime extends Context.Service<MetatoolRuntime, MetatoolRuntimeShape>()(
  "@tmnl/metatool/MetatoolRuntime"
) {}

// ── Default Node Layer ────────────────────────────────────────────

const runtimeError = (
  operation: RuntimeError["operation"],
  cause: unknown,
): RuntimeError => ({
  _tag: "RuntimeError",
  operation,
  message: cause instanceof Error ? cause.message : String(cause),
  cause,
})

const renderCommand = (input: RuntimeCommand): string => {
  const command = validateRuntimeCommand(input)
  if (typeof command === "string") return command
  if ("shell" in command) return command.shell
  const args = command.args?.join(" ") ?? ""
  return args.length > 0 ? `${command.cmd} ${args}` : command.cmd
}

/**
 * Node-backed runtime layer.
 *
 * This preserves existing behavior while moving execution behind a Schema-backed
 * Effect service. AgentOS/Rivet runtimes should implement this same service,
 * not patch metatool or the eval child directly.
 */
export function makeNodeRuntimeLayer(defaultCwd: string) {
  return Layer.effect(
    MetatoolRuntime,
    Effect.gen(function*() {
      const fs = yield* FileSystem
      const resolvePath = (path: string): string =>
        path.startsWith("/") ? path : join(defaultCwd, path)

      return MetatoolRuntime.of({
        read: (path: string) =>
          fs.readFileString(resolvePath(path)).pipe(
            Effect.mapError((cause) => runtimeError("read", cause)),
          ),

        write: (path: string, content: string) =>
          Effect.gen(function*() {
            const abs = resolvePath(path)
            yield* fs.makeDirectory(dirname(abs), { recursive: true }).pipe(
              Effect.catchTag("PlatformError", () => Effect.void),
              Effect.mapError((cause) => runtimeError("write", cause)),
            )
            yield* fs.writeFileString(abs, content).pipe(
              Effect.mapError((cause) => runtimeError("write", cause)),
            )
          }),

        exec: (input: RuntimeCommand, rawOptions?: RuntimeExecOptions) => {
          const command = renderCommand(input)
          const options = rawOptions == null ? undefined : validateRuntimeExecOptions(rawOptions)

          return Effect.tryPromise({
            try: () => new Promise<RuntimeExecResult>((resolve) => {
              exec(
                command,
                {
                  cwd: options?.cwd ?? defaultCwd,
                  encoding: "utf-8",
                  timeout: options?.timeoutMs ?? 15_000,
                  env: options?.env ? { ...process.env, ...options.env } : process.env,
                },
                (err, stdout, stderr) => {
                  const maybeError = err as (NodeJS.ErrnoException & { code?: number | string; killed?: boolean }) | null
                  const rawCode = maybeError?.code
                  const exitCode = typeof rawCode === "number" ? rawCode : rawCode == null ? 0 : 1
                  resolve(validateRuntimeExecResult({
                    stdout: stdout ?? "",
                    stderr: stderr ?? "",
                    exitCode,
                    command,
                    timedOut: maybeError?.killed === true,
                  }))
                },
              )
            }),
            catch: (cause) => runtimeError("exec", cause),
          })
        },

        spawn: (input: RuntimeCommand) =>
          Effect.fail(runtimeError(
            "spawn",
            new Error(`NodeRuntime does not expose managed spawn yet: ${renderCommand(input)}`),
          )),
      })
    }),
  )
}
