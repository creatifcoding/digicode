import type { ClientAction, ClientState, JsonObject, SessionSummary, TranscriptEntry } from './types';
import { initialClientState } from './types';

const generatedId = (() => { let count = 0; return (prefix: string) => `${prefix}-${Date.now()}-${++count}`; })();
const string = (value: unknown): string | undefined => typeof value === 'string' && value.length > 0 ? value : undefined;

function sessionFrom(value: unknown): SessionSummary | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const frame = value as JsonObject;
  const sessionId = string(frame.session_id) ?? string(frame.id);
  return sessionId ? {
    session_id: sessionId,
    title: string(frame.title) ?? string(frame.display_title),
    working_dir: string(frame.working_dir),
    status: string(frame.status),
  } : undefined;
}

function sessionsFrom(frame: JsonObject): SessionSummary[] {
  const raw = Array.isArray(frame.sessions) ? frame.sessions : Array.isArray(frame.items) ? frame.items : [];
  return raw.map(sessionFrom).filter((item): item is SessionSummary => Boolean(item));
}

function historyEntries(value: unknown): TranscriptEntry[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((message, index) => {
    if (!message || typeof message !== 'object') return [];
    const entry = message as JsonObject;
    const role = string(entry.role) ?? string(entry.kind) ?? 'system';
    const content = string(entry.content) ?? string(entry.text) ?? string(entry.message) ?? '';
    if (!content) return [];
    return [{ id: string(entry.id) ?? `history-${index}`, kind: role === 'user' ? 'user' : role === 'assistant' ? 'assistant' : 'system', text: content } satisfies TranscriptEntry];
  });
}

function appendAssistant(state: ClientState, delta: string, replace = false): ClientState {
  const last = state.transcript.at(-1);
  if (last?.kind === 'assistant') {
    const text = replace ? delta : last.text + delta;
    return { ...state, transcript: [...state.transcript.slice(0, -1), { ...last, text }], isStreaming: true };
  }
  return { ...state, transcript: [...state.transcript, { id: generatedId('assistant'), kind: 'assistant', text: delta }], isStreaming: true };
}

function updateTool(state: ClientState, frame: JsonObject, toolState: NonNullable<TranscriptEntry['toolState']>): ClientState {
  const id = string(frame.id) ?? string(frame.call_id);
  const name = string(frame.name) ?? 'tool';
  const output = string(frame.output) ?? string(frame.error);
  const existingIndex = id ? state.transcript.findIndex((entry) => entry.id === `tool-${id}`) : -1;
  const updated: TranscriptEntry = {
    id: id ? `tool-${id}` : generatedId('tool'), kind: 'tool', toolName: name,
    toolState, text: output ?? (toolState === 'done' ? 'Done' : name),
  };
  if (existingIndex >= 0) {
    const transcript = [...state.transcript];
    transcript[existingIndex] = { ...transcript[existingIndex]!, ...updated, id: transcript[existingIndex]!.id };
    return { ...state, transcript, isStreaming: toolState !== 'done' && toolState !== 'error' };
  }
  return { ...state, transcript: [...state.transcript, updated], isStreaming: toolState !== 'done' && toolState !== 'error' };
}

/** Reduces the tolerant gateway wire protocol into directly renderable UI state. */
export function clientReducer(state: ClientState, action: ClientAction): ClientState {
  if (action.type === 'reset') return initialClientState;
  if (action.type === 'connection') return {
    ...state, connection: action.state, reconnectAttempt: action.attempt ?? state.reconnectAttempt,
    error: action.error ?? (action.state === 'connected' ? undefined : state.error),
  };
  if (action.type === 'clear_error') return { ...state, error: undefined };
  if (action.type === 'select_session') return { ...state, activeSessionId: action.sessionId, transcript: [], isStreaming: false, error: undefined };
  if (action.type === 'optimistic_message') return {
    ...state, transcript: [...state.transcript, { id: generatedId('user'), kind: 'user', text: action.content }], isStreaming: true,
  };

  const frame = action.frame;
  const event = string(frame.type) ?? string(frame.event) ?? '';
  switch (event) {
    case 'session_list':
    case 'list_sessions': return { ...state, sessions: sessionsFrom(frame) };
    case 'session':
    case 'session_id': {
      const sessionId = string(frame.session_id) ?? string(frame.id);
      return sessionId ? { ...state, activeSessionId: sessionId } : state;
    }
    case 'history': return {
      ...state,
      activeSessionId: string(frame.session_id) ?? state.activeSessionId,
      transcript: historyEntries(frame.messages),
      isStreaming: false,
    };
    case 'text_delta': return appendAssistant(state, string(frame.text) ?? string(frame.delta) ?? '');
    case 'text_replace': return appendAssistant(state, string(frame.text) ?? '', true);
    case 'tool_start': return updateTool(state, frame, 'starting');
    case 'tool_exec': return updateTool(state, frame, 'executing');
    case 'tool_done': return updateTool(state, frame, frame.error ? 'error' : 'done');
    case 'message_end':
    case 'done': return { ...state, isStreaming: false };
    case 'swarm_status': return { ...state, swarmStatus: string(frame.status) ?? string(frame.message) };
    case 'error': return { ...state, error: string(frame.error) ?? string(frame.message) ?? 'Gateway request failed', isStreaming: false };
    default: return state;
  }
}

export { initialClientState };
