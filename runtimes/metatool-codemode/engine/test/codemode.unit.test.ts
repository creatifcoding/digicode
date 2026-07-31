/**
 * @module test/metatool.unit
 *
 * Core tests for createMetatool() — the SDK factory.
 * Tests plugin loading, method merging, eval sandbox, and dispose.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { createMetatool, type MetatoolPlugin } from '../src/index.js'
import { layer as sqliteNodeLayer } from '../src/adapters/sqlite-node.js'
import { NodeFileSystemLayer } from '../src/adapters/filesystem-node.js'
import { tmpdir } from 'node:os'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'

// ── Helpers ──────────────────────────────────────────────────────

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), 'metatool-test-'))
}

function makeTestPlugin(id: string, methods: Record<string, Function>): MetatoolPlugin {
  return { id, name: `Test Plugin ${id}`, methods }
}

// ── Tests ────────────────────────────────────────────────────────

describe('createMetatool', () => {
  let tmpDir: string
  let cleanup: (() => Promise<void>) | null = null

  afterEach(async () => {
    if (cleanup) await cleanup()
    cleanup = null
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true })
  })

  it('creates an instance with core methods', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')
    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // Core store methods exist
    expect(metatool.api.put).toBeDefined()
    expect(metatool.api.get).toBeDefined()
    expect(metatool.api.query).toBeDefined()
    expect(metatool.api.collections).toBeDefined()

    // Core DPA methods exist
    expect(metatool.api.define).toBeDefined()
    expect(metatool.api.call).toBeDefined()
    expect(metatool.api.fn).toBeDefined()

    // Core primitives exist
    expect(metatool.api.read).toBeDefined()
    expect(metatool.api.write).toBeDefined()
    expect(metatool.api.sh).toBeDefined()
  })

  it('loads plugins and merges methods', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const plugin = makeTestPlugin('test', {
      greet: (name: string) => `Hello ${name}`,
      add: (a: number, b: number) => a + b,
    })

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [plugin],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    expect(metatool.plugins).toContain('test')
    expect(metatool.api.greet).toBeDefined()
    expect(metatool.api.add).toBeDefined()
    // Core still there
    expect(metatool.api.put).toBeDefined()
  })

  it('eval sandbox executes code against merged API', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const plugin = makeTestPlugin('math', {
      multiply: (a: number, b: number) => a * b,
    })

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [plugin],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // Eval with core methods
    await metatool.eval('await cm.put("test", "key", { value: 42 })')
    const result = await metatool.eval('return await cm.get("test", "key")')
    expect(result).toEqual({ value: 42 })

    // Eval with plugin methods
    const mathResult = await metatool.eval('return cm.multiply(6, 7)')
    expect(mathResult).toBe(42)
  })

  it('detects plugin method collisions (last wins)', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const p1 = makeTestPlugin('first', { sharedMethod: () => 'from-first' })
    const p2 = makeTestPlugin('second', { sharedMethod: () => 'from-second' })

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [p1, p2],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // Last plugin wins
    const result = await metatool.eval('return cm.sharedMethod()')
    expect(result).toBe('from-second')
  })

  it('plugin setup receives core', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')
    let receivedCore = false

    const plugin: MetatoolPlugin = {
      id: 'setup-test',
      name: 'Setup Test',
      methods: {},
      setup: (core) => {
        receivedCore = true
        expect(core.store).toBeDefined()
        expect(core.procedures).toBeDefined()
        expect(core.cwd).toBe(tmpDir)
        expect(core.read).toBeDefined()
      },
    }

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [plugin],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    expect(receivedCore).toBe(true)
  })

  it('dispose calls plugin dispose hooks', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')
    let disposed = false

    const plugin: MetatoolPlugin = {
      id: 'dispose-test',
      name: 'Dispose Test',
      methods: {},
      dispose: () => { disposed = true },
    }

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [plugin],
      cwd: tmpDir,
    })

    await metatool.dispose()
    expect(disposed).toBe(true)
    cleanup = null // already disposed
  })

  it('core.read/write/sh work with cwd', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // write + read (async — Effect FileSystem-backed)
    await metatool.eval('await cm.write("test.txt", "hello metatool")')
    const content = await metatool.eval('return await cm.read("test.txt")')
    expect(content).toBe('hello metatool')

    // sh (async — Effect-wrapped execSync)
    const result = await metatool.eval('return await cm.sh("echo hello")')
    expect(result).toBe('hello')
  })
})
