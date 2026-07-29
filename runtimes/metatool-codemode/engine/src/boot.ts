/**
 * Guest boot surface for the Jcode codemode engine.
 *
 * Bundled (esbuild, single ESM file) and written into the AgentOS guest
 * filesystem. The guest bootstrap imports this module to assemble the
 * metatool instance against guest node:sqlite and the durable /data mount.
 */
export { createMetatool, type CreateMetatoolOptions } from "./index.js"
export { layer as sqliteNodeLayer } from "./adapters/sqlite-node.js"
export { NodeFileSystemLayer } from "./adapters/filesystem-node.js"
export { sanitizeForToolPayload, stringifyForToolContent } from "./clone-safe.js"
