import { afterEach, describe, expect, it } from "vitest"
import { mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { createMetatool, type MetatoolInstance } from "../src/index.js"
import { metatoolPlugin } from "../src/plugins/metatool.js"
import { layer as sqliteNodeLayer } from "../src/adapters/sqlite-node.js"
import { NodeFileSystemLayer } from "./_node-fs-layer.js"

const marker = "*(enrichment below this line)*"
const authorEntry = [
  "# Domain journal",
  "",
  "## E1 · 2026-07-13 · Original author record",
  "**Author**: operator, session original",
  "**Context**: I recorded the source context.",
  "I wrote this author body exactly.",
  marker,
  "",
].join("\n")
const secondEntry = [
  "## E2 · 2026-07-13 · Existing second entry",
  "**Author**: operator, session second",
  "**Context**: I keep this entry separate.",
  "Second author body.",
  marker,
  "",
].join("\n")

function makeFixture(): { cwd: string; journal: string } {
  const cwd = mkdtempSync(join(tmpdir(), "metatool-journal-"))
  const journal = join(cwd, "domain.journal.md")
  writeFileSync(journal, `${authorEntry}\n${secondEntry}`)
  return { cwd, journal }
}

async function makeMetatool(cwd: string): Promise<MetatoolInstance> {
  return createMetatool({
    sqlLayer: sqliteNodeLayer({ filename: join(cwd, "metatool.db") }),
    fsLayer: NodeFileSystemLayer,
    overlays: [metatoolPlugin(cwd, NodeFileSystemLayer)],
    cwd,
  })
}

describe("journal append APIs", () => {
  let cwd = ""
  let metatool: MetatoolInstance | undefined

  afterEach(async () => {
    await metatool?.dispose()
    metatool = undefined
    if (cwd) rmSync(cwd, { recursive: true, force: true })
    cwd = ""
  })

  it("appends an entry and enrichment through the eval API without changing author bytes above the marker", async () => {
    const fixture = makeFixture()
    cwd = fixture.cwd
    metatool = await makeMetatool(cwd)
    const original = readFileSync(fixture.journal, "utf8")
    const authorPrefix = original.slice(0, original.indexOf(marker) + marker.length)

    const entryEvidence = await metatool.eval(`return await cm.appendJournalEntry("domain.journal.md", {
      id: "E3",
      date: "2026-07-13",
      title: "API append",
      author: "metatool, session test",
      context: "I exercised the append boundary.",
      body: "The SDK appended this entry."
    })`)
    expect(entryEvidence).toMatchObject({
      path: "domain.journal.md",
      entryId: "E3",
      operation: "append-entry",
    })
    expect(entryEvidence.bytes).toBeGreaterThan(original.length)

    const enrichmentEvidence = await metatool.eval(`return await cm.appendJournalEnrichment("domain.journal.md", "E1", {
      date: "2026-07-13",
      agent: "delegate",
      body: "Verified references after the author record."
    })`)
    expect(enrichmentEvidence).toMatchObject({
      path: "domain.journal.md",
      entryId: "E1",
      operation: "append-enrichment",
    })

    const updated = readFileSync(fixture.journal, "utf8")
    expect(updated.slice(0, authorPrefix.length)).toBe(authorPrefix)
    expect(updated).toContain("### Enrichment · 2026-07-13 · delegate")
    expect(updated.indexOf("### Enrichment · 2026-07-13 · delegate")).toBeLessThan(updated.indexOf("## E2 ·"))
    expect(updated).toContain("## E3 · 2026-07-13 · API append")

    const entries = await metatool.eval('return await cm.journalEntries("domain.journal.md")')
    expect(entries).toEqual([
      { id: "E1", date: "2026-07-13", title: "Original author record", hasEnrichmentMarker: true },
      { id: "E2", date: "2026-07-13", title: "Existing second entry", hasEnrichmentMarker: true },
      { id: "E3", date: "2026-07-13", title: "API append", hasEnrichmentMarker: true },
    ])
  })

  it("refuses traversal, wrong suffix, and missing journal files", async () => {
    const fixture = makeFixture()
    cwd = fixture.cwd
    metatool = await makeMetatool(cwd)
    const outside = join(tmpdir(), `metatool-outside-${Date.now()}.journal.md`)
    writeFileSync(outside, authorEntry)
    symlinkSync(outside, join(cwd, "linked.journal.md"))


    await expect(metatool.eval('return await cm.journalEntries("../outside.journal.md")')).rejects.toMatchObject({ reason: "PathOutsideCwd" })
    await expect(metatool.eval('return await cm.journalEntries("notes.md")')).rejects.toMatchObject({ reason: "InvalidJournalPath" })
    await expect(metatool.eval('return await cm.journalEntries("missing.journal.md")')).rejects.toMatchObject({ reason: "FileNotFound" })
    await expect(metatool.eval('return await cm.journalEntries("linked.journal.md")')).rejects.toMatchObject({ reason: "PathOutsideCwd" })
    rmSync(outside, { force: true })
  })

  it("refuses duplicate IDs and malformed entry input", async () => {
    const fixture = makeFixture()
    cwd = fixture.cwd
    metatool = await makeMetatool(cwd)

    await expect(metatool.eval(`return await cm.appendJournalEntry("domain.journal.md", {
      id: "E1", date: "2026-07-13", title: "Duplicate", author: "author", context: "context", body: "body"
    })`)).rejects.toMatchObject({ reason: "DuplicateEntryId" })
    await expect(metatool.eval(`return await cm.appendJournalEntry("domain.journal.md", {
      id: "entry-3", date: "2026-02-30", title: "", author: "", context: "", body: ""
    })`)).rejects.toMatchObject({ reason: "InvalidInput" })
  })

  it("refuses enrichment without a target entry or its marker", async () => {
    const fixture = makeFixture()
    cwd = fixture.cwd
    metatool = await makeMetatool(cwd)
    const noMarker = join(cwd, "no-marker.journal.md")
    writeFileSync(noMarker, [
      "## E9 · 2026-07-13 · Missing marker",
      "**Author**: operator",
      "**Context**: I omitted the marker.",
      "Author body.",
      "",
    ].join("\n"))

    await expect(metatool.eval(`return await cm.appendJournalEnrichment("domain.journal.md", "E99", {
      date: "2026-07-13", agent: "delegate", body: "body"
    })`)).rejects.toMatchObject({ reason: "EntryNotFound" })
    await expect(metatool.eval(`return await cm.appendJournalEnrichment("no-marker.journal.md", "E9", {
      date: "2026-07-13", agent: "delegate", body: "body"
    })`)).rejects.toMatchObject({ reason: "EnrichmentMarkerNotFound" })
    await expect(metatool.eval(`return await cm.appendJournalEnrichment("domain.journal.md", "E1", {
      date: "invalid", agent: "", body: ""
    })`)).rejects.toMatchObject({ reason: "InvalidInput" })
  })
})
