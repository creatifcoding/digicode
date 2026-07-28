import { authenticatedWebSocketUrl } from './urls';
import type { ConnectionState, JsonObject } from '../protocol/types';

export const REQUEST_TIMEOUT_MS = 15_000;
const MAX_RECONNECT_DELAY_MS = 30_000;
const WEBSOCKET_OPEN = 1;

export function reconnectDelayMs(attempt: number, random: () => number = Math.random): number {
  const exponential = Math.min(MAX_RECONNECT_DELAY_MS, 1_000 * 2 ** Math.max(0, attempt - 1));
  return Math.round(exponential * (0.8 + random() * 0.4));
}

export class TransportStateMachine {
  state: ConnectionState = 'idle';
  attempt = 0;

  connecting(reconnecting = false): ConnectionState {
    this.state = reconnecting ? 'reconnecting' : 'connecting';
    return this.state;
  }

  opened(): ConnectionState {
    this.attempt = 0;
    this.state = 'connected';
    return this.state;
  }

  closed(reconnect: boolean): ConnectionState {
    this.attempt += reconnect ? 1 : 0;
    this.state = reconnect ? 'reconnecting' : 'disconnected';
    return this.state;
  }

  failed(): ConnectionState {
    this.state = 'error';
    return this.state;
  }
}

type PendingRequest = {
  resolve: (frame: JsonObject) => void;
  reject: (reason: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

export type GatewayTransportOptions = {
  wsUrl: string;
  token: string;
  onFrame: (frame: JsonObject) => void;
  onState: (state: ConnectionState, attempt: number, error?: string) => void;
  onOpen?: () => void;
  webSocketFactory?: (url: string) => WebSocket;
  random?: () => number;
};

/** Authenticated JSON socket with request correlation and capped reconnect backoff. */
export class GatewayTransport {
  private readonly pending = new Map<number, PendingRequest>();
  private readonly machine = new TransportStateMachine();
  private readonly createSocket: (url: string) => WebSocket;
  private nextRequestId = 1;
  private socket?: WebSocket;
  private reconnectTimer?: ReturnType<typeof setTimeout>;
  private stopped = false;

  constructor(private readonly options: GatewayTransportOptions) {
    this.createSocket = options.webSocketFactory ?? ((url) => new WebSocket(url));
  }

  start(): void {
    this.stopped = false;
    this.connect(false);
  }

  stop(): void {
    this.stopped = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = undefined;
    this.socket?.close();
    this.socket = undefined;
    this.rejectPending(new Error('Connection stopped.'));
    this.options.onState(this.machine.closed(false), this.machine.attempt);
  }

  send(payload: JsonObject): number {
    const id = typeof payload.id === 'number' ? payload.id : this.nextRequestId++;
    this.sendFrame({ ...payload, id });
    return id;
  }

  request(payload: JsonObject, timeoutMs = REQUEST_TIMEOUT_MS): Promise<JsonObject> {
    const id = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error('Gateway request timed out.'));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
      try {
        this.sendFrame({ ...payload, id });
      } catch (error) {
        clearTimeout(timeout);
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error('Could not send gateway request.'));
      }
    });
  }

  private connect(reconnecting: boolean): void {
    if (this.stopped) return;
    this.options.onState(this.machine.connecting(reconnecting), this.machine.attempt);
    try {
      this.socket = this.createSocket(authenticatedWebSocketUrl(this.options.wsUrl, this.options.token));
      this.socket.onopen = () => {
        this.options.onState(this.machine.opened(), this.machine.attempt);
        this.options.onOpen?.();
      };
      this.socket.onmessage = (event) => this.receive(event.data);
      this.socket.onerror = () => {
        if (!this.stopped) this.options.onState(this.machine.failed(), this.machine.attempt, 'WebSocket connection failed.');
      };
      this.socket.onclose = () => this.scheduleReconnect();
    } catch {
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    if (this.stopped || this.reconnectTimer) return;
    this.options.onState(this.machine.closed(true), this.machine.attempt);
    this.rejectPending(new Error('Connection closed before request completed.'));
    const delay = reconnectDelayMs(this.machine.attempt, this.options.random);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = undefined;
      this.connect(true);
    }, delay);
  }

  private sendFrame(frame: JsonObject): void {
    if (!this.socket || this.socket.readyState !== WEBSOCKET_OPEN) {
      throw new Error('Gateway is not connected.');
    }
    this.socket.send(JSON.stringify(frame));
  }

  private receive(data: unknown): void {
    if (typeof data !== 'string') return;
    let frame: JsonObject;
    try {
      const parsed: unknown = JSON.parse(data);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
      frame = parsed as JsonObject;
    } catch {
      return;
    }
    this.options.onFrame(frame);
    const id = typeof frame.id === 'number' ? frame.id : undefined;
    if (id === undefined) return;
    const pending = this.pending.get(id);
    if (!pending) return;
    this.pending.delete(id);
    clearTimeout(pending.timeout);
    if (frame.type === 'error') {
      pending.reject(new Error(typeof frame.message === 'string' ? frame.message : 'Gateway request failed.'));
    } else {
      pending.resolve(frame);
    }
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
    this.pending.clear();
  }
}
