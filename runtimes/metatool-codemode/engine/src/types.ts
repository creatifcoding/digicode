/**
 * @module types
 *
 * Core type definitions for @tmnl/metatool.
 */

import type { StoreApi } from "./store/api.js"
import type { ProcedureApi } from "./store/procedures.js"
import type { OverlayManager } from "./overlay.js"
import type { MetatoolRuntime } from "./services/MetatoolRuntime.js"
import type { RuntimeCommand, RuntimeExecOptions, RuntimeExecResult } from "./schemas/runtime.js"

// ── MetatoolCore ─────────────────────────────────────────────────

/**
 * The core infrastructure that plugins receive during setup.
 * Everything domain-agnostic lives here.
 */
export interface MetatoolCore {
  /** RLM persistent store */
  readonly store: StoreApi

  /** DPA stored procedures */
  readonly procedures: ProcedureApi

  /** Working directory */
  readonly cwd: string

  /** Read a file (cwd-relative or absolute). Async — must await. */
  readonly read: (path: string) => Promise<string>

  /** Write a file (cwd-relative or absolute, auto-creates parent dirs). Async — must await. */
  readonly write: (path: string, content: string) => Promise<void>

  /** Execute a shell command (compatibility wrapper over runtime.exec). Async — must await. */
  readonly sh: (cmd: string) => Promise<string>

  /** Execute through the configured runtime boundary. Async — must await. */
  readonly exec: (command: RuntimeCommand, options?: RuntimeExecOptions) => Promise<RuntimeExecResult>

  /** Configured runtime service tag, exposed for advanced overlay composition. */
  readonly runtime: typeof MetatoolRuntime
}

// ── MetatoolConfig ───────────────────────────────────────────────

export interface MetatoolConfig {
  /** Working directory for file/shell ops */
  readonly cwd: string

  /** Database file path (for SQLite adapters) */
  readonly dbPath?: string
}

// ── MetatoolInstance ──────────────────────────────────────────────

/**
 * The assembled metatool instance returned by createMetatool().
 */
export interface MetatoolInstance {
  /** The merged API object — core + all overlay methods (mutated in-place on recompile) */
  readonly api: Record<string, Function>

  /** Always-fresh API snapshot — reads from overlayManager.compiled() on every call */
  getApi(): Record<string, Function>

  /** Evaluate code in the sandbox against the merged API */
  readonly eval: (code: string) => Promise<any>

  /** The underlying core — for programmatic access */
  readonly core: MetatoolCore

  /** Loaded overlay IDs (backward compat — same as overlays.active().map(o => o.id)) */
  readonly plugins: ReadonlyArray<string>

  /** Overlay manager — load, unload, switch overlays dynamically */
  readonly overlays: OverlayManager

  /** Dispose all resources */
  readonly dispose: () => Promise<void>
}
