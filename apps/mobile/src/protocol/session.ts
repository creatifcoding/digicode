import type { JsonObject } from './types';

export const DEFAULT_SESSION_WORKING_DIR = '/';

export function normalizeSessionWorkingDir(workingDir?: string): string {
  const normalized = workingDir?.trim();
  return normalized && normalized.startsWith('/') ? normalized : DEFAULT_SESSION_WORKING_DIR;
}

/** Build the stateful subscribe request required by the gateway server. */
export function sessionSubscribeRequest(sessionId: string, workingDir?: string): JsonObject {
  return {
    type: 'subscribe',
    target_session_id: sessionId,
    working_dir: normalizeSessionWorkingDir(workingDir),
  };
}

/** Resuming an existing session returns history as part of the subscribe reply. */
export function needsHistoryAfterSubscribe(response: JsonObject): boolean {
  return response.type !== 'history';
}
