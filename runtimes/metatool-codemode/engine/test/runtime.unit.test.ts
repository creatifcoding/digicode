import { describe, expect, it } from 'vitest'
import * as Effect from 'effect/Effect'
import * as Layer from 'effect/Layer'
import * as ManagedRuntime from 'effect/ManagedRuntime'
import { tmpdir } from 'node:os'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { NodeFileSystemLayer } from '../src/adapters/filesystem-node.js'
import { MetatoolRuntime, makeNodeRuntimeLayer } from '../src/services/MetatoolRuntime.js'
import {
  validateRuntimeCommand,
  validateRuntimeExecOptions,
  validateRuntimeExecResult,
} from '../src/schemas/runtime.js'
import { createMetatool } from '../src/index.js'
import { layer as sqliteNodeLayer } from '../src/adapters/sqlite-node.js'

describe('schema-first runtime boundary', () => {
  it('validates command, option, and result payloads', () => {
    expect(validateRuntimeCommand({ shell: 'echo ok' })).toEqual({ shell: 'echo ok' })
    expect(validateRuntimeCommand({ cmd: 'echo', args: ['ok'] })).toEqual({ cmd: 'echo', args: ['ok'] })
    expect(validateRuntimeExecOptions({ cwd: '/tmp', timeoutMs: 1000, env: { X: '1' } })).toEqual({ cwd: '/tmp', timeoutMs: 1000, env: { X: '1' } })
    expect(validateRuntimeExecResult({ stdout: 'ok\n', stderr: '', exitCode: 0, command: 'echo ok', timedOut: false })).toEqual({ stdout: 'ok\n', stderr: '', exitCode: 0, command: 'echo ok', timedOut: false })
  })

  it('runs Node runtime through the MetatoolRuntime service', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'metatool-runtime-'))
    const runtime = ManagedRuntime.make(makeNodeRuntimeLayer(dir).pipe(Layer.provide(NodeFileSystemLayer)))
    try {
      await runtime.runPromise(MetatoolRuntime.pipe(
        Effect.flatMap((rt) => rt.write('hello.txt', 'hello runtime')),
      ))
      const content = await runtime.runPromise(MetatoolRuntime.pipe(
        Effect.flatMap((rt) => rt.read('hello.txt')),
      ))
      const result = await runtime.runPromise(MetatoolRuntime.pipe(
        Effect.flatMap((rt) => rt.exec({ shell: 'echo runtime' })),
      ))
      expect(content).toBe('hello runtime')
      expect(result.stdout.trim()).toBe('runtime')
      expect(result.exitCode).toBe(0)
    } finally {
      await runtime.dispose()
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('exposes exec on createMetatool without breaking sh compatibility', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'metatool-runtime-api-'))
    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: join(dir, 'test.db') }),
      fsLayer: NodeFileSystemLayer,
      cwd: dir,
    })
    try {
      const sh = await metatool.eval('return await cm.sh("echo hello")')
      const exec = await metatool.eval('return await cm.exec({ shell: "echo exec" })')
      expect(sh).toBe('hello')
      expect(exec.stdout.trim()).toBe('exec')
      expect(exec.exitCode).toBe(0)
    } finally {
      await metatool.dispose()
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
