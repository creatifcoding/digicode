/**
 * @module schemas/runtime
 *
 * Schema-first runtime contract for metatool/metatool execution boundaries.
 *
 * Keep wire/domain shapes here. Services derive TypeScript types from Schema
 * instead of defining freehand interfaces first.
 */

import * as Schema from "effect/Schema"

export const RuntimeShellCommand = Schema.Struct({
  shell: Schema.String,
})
export type RuntimeShellCommand = typeof RuntimeShellCommand.Type

export const RuntimeArgvCommand = Schema.Struct({
  cmd: Schema.String,
  args: Schema.optional(Schema.Array(Schema.String)),
})
export type RuntimeArgvCommand = typeof RuntimeArgvCommand.Type

export const RuntimeCommand = Schema.Union([
  Schema.String,
  RuntimeShellCommand,
  RuntimeArgvCommand,
])
export type RuntimeCommand = typeof RuntimeCommand.Type

export const RuntimeExecOptions = Schema.Struct({
  cwd: Schema.optional(Schema.String),
  timeoutMs: Schema.optional(Schema.Number),
  env: Schema.optional(Schema.Record(Schema.String, Schema.String)),
})
export type RuntimeExecOptions = typeof RuntimeExecOptions.Type

export const RuntimeExecResult = Schema.Struct({
  stdout: Schema.String,
  stderr: Schema.String,
  exitCode: Schema.Number,
  command: Schema.String,
  timedOut: Schema.Boolean,
})
export type RuntimeExecResult = typeof RuntimeExecResult.Type

export const RuntimeError = Schema.Struct({
  _tag: Schema.Literal("RuntimeError"),
  operation: Schema.Literals(["read", "write", "exec", "spawn"]),
  message: Schema.String,
  cause: Schema.optional(Schema.Unknown),
})
export type RuntimeError = typeof RuntimeError.Type

export const RuntimeSpawnOptions = RuntimeExecOptions
export type RuntimeSpawnOptions = RuntimeExecOptions

export const RuntimeProcessDescriptor = Schema.Struct({
  id: Schema.String,
  command: Schema.String,
})
export type RuntimeProcessDescriptor = typeof RuntimeProcessDescriptor.Type

const decodeRuntimeCommand = Schema.decodeUnknownSync(RuntimeCommand)
const decodeRuntimeExecOptions = Schema.decodeUnknownSync(RuntimeExecOptions)
const decodeRuntimeExecResult = Schema.decodeUnknownSync(RuntimeExecResult)

export const validateRuntimeCommand = (command: unknown): RuntimeCommand =>
  decodeRuntimeCommand(command)

export const validateRuntimeExecOptions = (options: unknown): RuntimeExecOptions =>
  decodeRuntimeExecOptions(options)

export const validateRuntimeExecResult = (result: unknown): RuntimeExecResult =>
  decodeRuntimeExecResult(result)
