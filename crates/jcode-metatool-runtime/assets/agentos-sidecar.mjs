import { AgentOs } from "@rivet-dev/agentos-core";

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const request = JSON.parse(Buffer.concat(chunks).toString("utf8"));

const runtime = await AgentOs.create({
  permissions: {
    fs: "allow",
    network: "deny",
    childProcess: {
      default: "deny",
      rules: [{ mode: "allow", operations: ["spawn"], patterns: ["node"] }],
    },
    process: "allow",
    env: "allow",
    binding: "deny",
  },
  limits: {
    resources: {
      maxProcesses: 4,
      maxOpenFds: 32,
      maxSockets: 0,
      maxFilesystemBytes: 32 * 1024 * 1024,
      maxInodeCount: 4096,
    },
    process: {
      pendingStdinBytes: 64 * 1024,
      pendingEventCount: 256,
      pendingEventBytes: 256 * 1024,
    },
    jsRuntime: {
      v8HeapLimitMb: request.limits.heap_mb,
      cpuTimeLimitMs: request.limits.cpu_time_ms,
      wallClockLimitMs: request.limits.wall_time_ms,
    },
  },
});

const started = Date.now();
try {
  const result = await runtime.javascript.evaluate(request.source, {
    inputs: request.inputs,
    timeoutMs: request.limits.wall_time_ms,
  });
  process.stdout.write(JSON.stringify({
    protocol_version: 1,
    id: request.id,
    duration_ms: Date.now() - started,
    result,
  }));
} catch (error) {
  process.stdout.write(JSON.stringify({
    protocol_version: 1,
    id: request.id,
    duration_ms: Date.now() - started,
    error: {
      name: error?.name ?? "Error",
      message: error?.message ?? String(error),
      code: error?.code ?? null,
    },
  }));
} finally {
  await runtime.dispose();
}
