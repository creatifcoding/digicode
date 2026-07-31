/**
 * @module test/metatool-integration
 *
 * Integration test — metatoolPlugin (now MetatoolOverlay) loaded into createMetatool.
 * Verifies the full overlay lifecycle: load, method merge, eval access, dispose.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { createMetatool } from '../src/index.js'
import { metatoolPlugin } from '../src/plugins/metatool.js'
import { layer as sqliteNodeLayer } from '../src/adapters/sqlite-node.js'
import { NodeFileSystemLayer } from './_node-fs-layer.js'
import { tmpdir } from 'node:os'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

function makeTmpDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'metatool-ms-int-'))
  const skillDir = join(dir, '.pi', 'skills', 'test-skill')
  mkdirSync(skillDir, { recursive: true })
  writeFileSync(join(skillDir, 'SKILL.md'), '---\ngoverned-by: metatool\n---\n# test-skill\n')
  return dir
}

describe('createMetatool + metatoolPlugin (overlay)', () => {
  let tmpDir: string
  let cleanup: (() => Promise<void>) | null = null

  afterEach(async () => {
    if (cleanup) await cleanup()
    cleanup = null
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true })
  })

  it('loads metatool overlay and merges methods', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      overlays: [metatoolPlugin(tmpDir, NodeFileSystemLayer)],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // Core methods present
    expect(metatool.api.put).toBeDefined()
    expect(metatool.api.get).toBeDefined()
    expect(metatool.api.define).toBeDefined()

    // Domain methods merged from metatool overlay
    expect(metatool.api.discover).toBeDefined()
    expect(metatool.api.inspect).toBeDefined()
    expect(metatool.api.audit).toBeDefined()
    expect(metatool.api.conformance).toBeDefined()
    expect(metatool.api.profile).toBeDefined()

    // Overlay listed
    expect(metatool.plugins).toContain('metatool')
  })

  it('domain methods accessible via eval sandbox', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      overlays: [metatoolPlugin(tmpDir, NodeFileSystemLayer)],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // discover() returns array via Effect runtime
    const result = await metatool.eval('return await cm.discover()')
    expect(Array.isArray(result)).toBe(true)
    expect(result.length).toBeGreaterThanOrEqual(1)
  })

  it('core store + domain overlay coexist without collision', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      overlays: [metatoolPlugin(tmpDir, NodeFileSystemLayer)],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    // Store operation
    await metatool.eval('await cm.put("test", "k1", { value: 1 })')
    const storeResult = await metatool.eval('return await cm.get("test", "k1")')
    expect(storeResult).toEqual({ value: 1 })

    // Domain operation (in same sandbox)
    const skills = await metatool.eval('return await cm.discover()')
    expect(Array.isArray(skills)).toBe(true)

    // Both work without interfering
    const storeStillWorks = await metatool.eval('return await cm.get("test", "k1")')
    expect(storeStillWorks).toEqual({ value: 1 })
  })

  it('api method count = core + domain + overlay ops', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      overlays: [metatoolPlugin(tmpDir, NodeFileSystemLayer)],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    const allMethods = Object.keys(metatool.api)
    // Core ~37 + metatool 21 + overlay ops 5 → ~63 total
    expect(allMethods.length).toBeGreaterThanOrEqual(40)

    // Overlay management methods exist
    expect(metatool.api.loadOverlay).toBeDefined()
    expect(metatool.api.unloadOverlay).toBeDefined()
    expect(metatool.api.switchOverlay).toBeDefined()
    expect(metatool.api.overlays).toBeDefined()
    expect(metatool.api.hasOverlay).toBeDefined()
  })

  it('overlays manager is exposed', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      overlays: [metatoolPlugin(tmpDir, NodeFileSystemLayer)],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    expect(metatool.overlays).toBeDefined()
    expect(metatool.overlays.has('metatool')).toBe(true)
    expect(metatool.overlays.size).toBe(1)
    expect(metatool.overlays.active()).toEqual([
      { id: 'metatool', name: 'Skill Governance', version: undefined },
    ])
  })

  it('legacy plugins: [] still works (backward compat)', async () => {
    tmpDir = makeTmpDir()
    const dbPath = join(tmpDir, 'test.db')

    // Use legacy plugins: [] — should auto-wrap to overlays
    const metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: dbPath }),
      fsLayer: NodeFileSystemLayer,
      plugins: [metatoolPlugin(tmpDir, NodeFileSystemLayer) as any],
      cwd: tmpDir,
    })
    cleanup = metatool.dispose

    expect(metatool.api.discover).toBeDefined()
    expect(metatool.overlays.has('metatool')).toBe(true)
  })
})
