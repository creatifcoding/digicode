import { GatewayTransport, TransportStateMachine, reconnectDelayMs } from '../src/network/transport';

class MockSocket {
  static OPEN = 1;
  readyState = MockSocket.OPEN;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  sent: string[] = [];
  constructor(readonly url: string) {}
  send(data: string) { this.sent.push(data); }
  close() { this.onclose?.(); }
}

describe('transport state', () => {
  it('uses capped jittered exponential backoff', () => {
    expect(reconnectDelayMs(1, () => 0)).toBe(800);
    expect(reconnectDelayMs(99, () => 1)).toBe(36000);
  });

  it('tracks close and open lifecycle', () => {
    const machine = new TransportStateMachine();
    expect(machine.connecting()).toBe('connecting');
    expect(machine.closed(true)).toBe('reconnecting');
    expect(machine.attempt).toBe(1);
    expect(machine.opened()).toBe('connected');
    expect(machine.attempt).toBe(0);
  });

  it('correlates a response to its request id', async () => {
    let socket: MockSocket | undefined;
    const transport = new GatewayTransport({ wsUrl: 'ws://host/ws', token: 'secret', onFrame: jest.fn(), onState: jest.fn(), webSocketFactory: (url) => (socket = new MockSocket(url)) as unknown as WebSocket });
    transport.start();
    socket!.onopen?.();
    const response = transport.request({ type: 'get_history' });
    const request = JSON.parse(socket!.sent[0]!);
    socket!.onmessage?.({ data: JSON.stringify({ type: 'history', id: request.id, messages: [] }) });
    await expect(response).resolves.toMatchObject({ type: 'history', id: request.id });
    expect(socket!.url).toContain('token=secret');
    transport.stop();
  });
});
