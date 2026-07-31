/**
 * @module plugins/metatool-services/journal-service
 *
 * Append-only operations for domain-colocated Digimason journals.
 */

import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import { FileSystem } from "effect/FileSystem"
import { basename, isAbsolute, relative, resolve, sep } from "node:path"
import { SkillConfig } from "./skill-config.js"
import { JournalError } from "./errors.js"

export const ENRICHMENT_MARKER = "*(enrichment below this line)*"

export interface JournalEntry {
  readonly id: string
  readonly date: string
  readonly title: string
  readonly hasEnrichmentMarker: boolean
}

export interface AppendJournalEntryInput {
  readonly id: string
  readonly date: string
  readonly title: string
  readonly author: string
  readonly context: string
  readonly body: string
}

export interface AppendJournalEnrichmentInput {
  readonly date: string
  readonly agent: string
  readonly body: string
}

export interface JournalAppendEvidence {
  readonly path: string
  readonly entryId: string
  readonly bytes: number
  readonly operation: "append-entry" | "append-enrichment"
}

export interface JournalServiceShape {
  readonly journalEntries: (path: string) => Effect.Effect<ReadonlyArray<JournalEntry>, JournalError>
  readonly appendJournalEntry: (
    path: string,
    input: AppendJournalEntryInput,
  ) => Effect.Effect<JournalAppendEvidence, JournalError>
  readonly appendJournalEnrichment: (
    path: string,
    entryId: string,
    input: AppendJournalEnrichmentInput,
  ) => Effect.Effect<JournalAppendEvidence, JournalError>
}

export class JournalService extends Context.Service<JournalService, JournalServiceShape>()(
  "@gbg/metatool-metatool/JournalService",
) {}

interface ParsedJournalEntry extends JournalEntry {
  readonly start: number
  readonly end: number
}

const entryHeading = /^## (E\d+) · (\d{4}-\d{2}-\d{2}) · ([^\r\n]+)$/gm
const entryId = /^E\d+$/
const isoDate = /^\d{4}-\d{2}-\d{2}$/

/** Deterministically parse only canonical level-two journal entry headings. */
export function parseJournalEntries(content: string): ReadonlyArray<ParsedJournalEntry> {
  const matches = Array.from(content.matchAll(entryHeading))
  return matches.map((match, index) => {
    const start = match.index ?? 0
    return {
      id: match[1],
      date: match[2],
      title: match[3],
      start,
      end: index + 1 < matches.length ? (matches[index + 1].index ?? content.length) : content.length,
      hasEnrichmentMarker: content.slice(start, index + 1 < matches.length ? (matches[index + 1].index ?? content.length) : content.length)
        .includes(ENRICHMENT_MARKER),
    }
  })
}

function isValidDate(value: string): boolean {
  if (!isoDate.test(value)) return false
  const date = new Date(`${value}T00:00:00.000Z`)
  return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === value
}

function isJournalPath(path: string): boolean {
  return path.endsWith(".journal.md") || basename(path) === "infra-journal.md"
}

function validText(value: unknown, field: string, singleLine = false): string | JournalError {
  if (typeof value !== "string" || value.trim().length === 0) {
    return new JournalError({ reason: "InvalidInput", detail: `${field} must be nonempty` })
  }
  if (singleLine && /[\r\n]/.test(value)) {
    return new JournalError({ reason: "InvalidInput", detail: `${field} must be one line` })
  }
  return value
}

function validateEntryInput(input: AppendJournalEntryInput): JournalError | undefined {
  if (typeof input !== "object" || input === null) {
    return new JournalError({ reason: "InvalidInput", detail: "entry input is required" })
  }
  if (typeof input.id !== "string" || !entryId.test(input.id)) {
    return new JournalError({ reason: "InvalidInput", detail: "id must match E<digits>" })
  }
  if (typeof input.date !== "string" || !isValidDate(input.date)) {
    return new JournalError({ reason: "InvalidInput", detail: "date must be ISO YYYY-MM-DD" })
  }
  for (const [field, singleLine] of [["title", true], ["author", true], ["context", true], ["body", false]] as const) {
    const error = validText(input[field], field, singleLine)
    if (error instanceof JournalError) return error
  }
  return undefined
}

function validateEnrichmentInput(input: AppendJournalEnrichmentInput): JournalError | undefined {
  if (typeof input !== "object" || input === null) {
    return new JournalError({ reason: "InvalidInput", detail: "enrichment input is required" })
  }
  if (typeof input.date !== "string" || !isValidDate(input.date)) {
    return new JournalError({ reason: "InvalidInput", detail: "date must be ISO YYYY-MM-DD" })
  }
  for (const [field, singleLine] of [["agent", true], ["body", false]] as const) {
    const error = validText(input[field], field, singleLine)
    if (error instanceof JournalError) return error
  }
  return undefined
}

function appendBlock(content: string, block: string): string {
  if (content.length === 0) return block
  return `${content}${content.endsWith("\n") ? "" : "\n"}\n${block}`
}

function formatEntry(input: AppendJournalEntryInput): string {
  return [
    `## ${input.id} · ${input.date} · ${input.title}`,
    `**Author**: ${input.author}`,
    `**Context**: ${input.context}`,
    input.body,
    ENRICHMENT_MARKER,
    "",
  ].join("\n")
}

function formatEnrichment(input: AppendJournalEnrichmentInput): string {
  return [
    `### Enrichment · ${input.date} · ${input.agent}`,
    input.body,
    "",
  ].join("\n")
}

export const JournalServiceLive = Layer.effect(
  JournalService,
  Effect.gen(function*() {
    const config = yield* SkillConfig
    const fs = yield* FileSystem

    const resolveJournalPath = (path: string): Effect.Effect<{ absolute: string; display: string }, JournalError> =>
      Effect.gen(function*() {
        const absolute = resolve(config.cwd, path)
        const rel = relative(config.cwd, absolute)
        if (rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
          return yield* Effect.fail(new JournalError({ reason: "PathOutsideCwd", path, detail: "path must resolve inside cwd" }))
        }
        if (!isJournalPath(absolute)) {
          return yield* Effect.fail(new JournalError({ reason: "InvalidJournalPath", path, detail: "expected *.journal.md or infra-journal.md" }))
        }
        const stat = yield* fs.stat(absolute).pipe(
          Effect.mapError(error => new JournalError({ reason: "FileNotFound", path, detail: error.message })),
        )
        if (stat.type !== "File") {
          return yield* Effect.fail(new JournalError({ reason: "FileNotFound", path, detail: "journal path is not a file" }))
        }
        const root = yield* fs.realPath(config.cwd).pipe(
          Effect.mapError(error => new JournalError({ reason: "FileNotFound", path: config.cwd, detail: error.message })),
        )
        const canonical = yield* fs.realPath(absolute).pipe(
          Effect.mapError(error => new JournalError({ reason: "FileNotFound", path, detail: error.message })),
        )
        const canonicalRel = relative(root, canonical)
        if (canonicalRel === ".." || canonicalRel.startsWith(`..${sep}`) || isAbsolute(canonicalRel)) {
          return yield* Effect.fail(new JournalError({ reason: "PathOutsideCwd", path, detail: "journal target resolves outside cwd" }))
        }
        return { absolute: canonical, display: rel || basename(absolute) }
      })

    const readJournal = (path: string) => Effect.gen(function*() {
      const resolved = yield* resolveJournalPath(path)
      const content = yield* fs.readFileString(resolved.absolute).pipe(
        Effect.mapError(error => new JournalError({ reason: "FileNotFound", path, detail: error.message })),
      )
      return { ...resolved, content }
    })

    const atomicWrite = (path: string, content: string): Effect.Effect<number, JournalError> => {
      const temporary = `${path}.metatool-${Date.now()}-${Math.random().toString(36).slice(2)}.tmp`
      return fs.writeFileString(temporary, content).pipe(
        Effect.mapError(error => new JournalError({ reason: "WriteFailed", path, detail: error.message })),
        Effect.andThen(fs.rename(temporary, path)),
        Effect.mapError(error => new JournalError({ reason: "WriteFailed", path, detail: error.message })),
        Effect.as(new TextEncoder().encode(content).byteLength),
      )
    }

    return JournalService.of({
      journalEntries: (path) => readJournal(path).pipe(
        Effect.map(({ content }) => parseJournalEntries(content).map(({ start: _start, end: _end, ...entry }) => entry)),
      ),

      appendJournalEntry: (path, input) => Effect.gen(function*() {
        const inputError = validateEntryInput(input)
        if (inputError) return yield* Effect.fail(inputError)
        const journal = yield* readJournal(path)
        const entries = parseJournalEntries(journal.content)
        if (entries.some(entry => entry.id === input.id)) {
          return yield* Effect.fail(new JournalError({ reason: "DuplicateEntryId", path, detail: input.id }))
        }
        const bytes = yield* atomicWrite(journal.absolute, appendBlock(journal.content, formatEntry(input)))
        return { path: journal.display, entryId: input.id, bytes, operation: "append-entry" } as const
      }),

      appendJournalEnrichment: (path, id, input) => Effect.gen(function*() {
        if (typeof id !== "string" || !entryId.test(id)) {
          return yield* Effect.fail(new JournalError({ reason: "InvalidInput", path, detail: "entryId must match E<digits>" }))
        }
        const inputError = validateEnrichmentInput(input)
        if (inputError) return yield* Effect.fail(inputError)
        const journal = yield* readJournal(path)
        const entry = parseJournalEntries(journal.content).find(candidate => candidate.id === id)
        if (!entry) return yield* Effect.fail(new JournalError({ reason: "EntryNotFound", path, detail: id }))
        const entryContent = journal.content.slice(entry.start, entry.end)
        const markerOffset = entryContent.indexOf(ENRICHMENT_MARKER)
        if (markerOffset === -1) {
          return yield* Effect.fail(new JournalError({ reason: "EnrichmentMarkerNotFound", path, detail: id }))
        }
        const beforeEntry = journal.content.slice(0, entry.end)
        const afterEntry = journal.content.slice(entry.end)
        const updatedEntry = appendBlock(beforeEntry, formatEnrichment(input))
        const bytes = yield* atomicWrite(journal.absolute, `${updatedEntry}${afterEntry}`)
        return { path: journal.display, entryId: id, bytes, operation: "append-enrichment" } as const
      }),
    })
  }),
)
