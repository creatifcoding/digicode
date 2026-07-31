import { authenticatedWebSocketUrl, normalizeGatewayUrl } from '../src/network/urls';

describe('normalizeGatewayUrl', () => {
  it('adds a scheme and canonical paths', () => {
    expect(normalizeGatewayUrl('jcode.local:8787')).toEqual({ httpBase: 'http://jcode.local:8787', wsUrl: 'ws://jcode.local:8787/ws' });
  });

  it('preserves secure transport', () => {
    expect(normalizeGatewayUrl('https://host.example')).toEqual({ httpBase: 'https://host.example', wsUrl: 'wss://host.example/ws' });
  });

  it('rejects paths and token-bearing input', () => {
    expect(() => normalizeGatewayUrl('https://host.example/ws')).toThrow('path');
    expect(() => normalizeGatewayUrl('https://host.example?token=secret')).toThrow('query');
  });

  it('adds token only to the WebSocket URL', () => {
    expect(authenticatedWebSocketUrl('wss://host.example/ws', 't+1')).toBe('wss://host.example/ws?token=t%2B1');
  });
});
