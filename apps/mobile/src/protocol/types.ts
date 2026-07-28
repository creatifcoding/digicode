export type JsonObject = Record<string, unknown>;

export type SessionSummary = {
  session_id: string;
  title?: string | null;
  working_dir?: string | null;
  status?: string | null;
};

export type TranscriptEntry = {
  id: string;
  kind: 'user' | 'assistant' | 'tool' | 'system';
  text: string;
  toolName?: string;
  toolState?: 'starting' | 'executing' | 'done' | 'error';
};

export type ConnectionState = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'disconnected' | 'error';

export type ClientState = {
  connection: ConnectionState;
  reconnectAttempt: number;
  sessions: SessionSummary[];
  activeSessionId?: string;
  transcript: TranscriptEntry[];
  isStreaming: boolean;
  swarmStatus?: string;
  error?: string;
};

export const initialClientState: ClientState = {
  connection: 'idle',
  reconnectAttempt: 0,
  sessions: [],
  transcript: [],
  isStreaming: false,
};

export type ClientAction =
  | { type: 'connection'; state: ConnectionState; attempt?: number; error?: string }
  | { type: 'wire'; frame: JsonObject }
  | { type: 'optimistic_message'; content: string }
  | { type: 'select_session'; sessionId: string }
  | { type: 'reset' }
  | { type: 'clear_error' };

export type PairRequest = { code: string; device_id: string; device_name: string };
export type PairResponse = { token: string; server_name: string; server_version: string };
export type Credential = GatewayUrlsCredential & PairResponse;
export type GatewayUrlsCredential = { gateway: string };
