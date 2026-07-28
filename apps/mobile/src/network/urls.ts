export type GatewayUrls = {
  /** Canonical origin used for pairing. */
  httpBase: string;
  /** Canonical endpoint used for WebSocket connections. */
  wsUrl: string;
};

/**
 * Normalizes a user-entered Jcode gateway address without accepting paths,
 * credentials, or query strings that could make token handling ambiguous.
 */
export function normalizeGatewayUrl(input: string): GatewayUrls {
  const trimmed = input.trim();
  if (!trimmed) throw new Error('Enter a gateway host or URL.');

  const candidate = /^[a-z][a-z\d+.-]*:\/\//i.test(trimmed)
    ? trimmed
    : `http://${trimmed}`;
  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    throw new Error('Gateway address is not a valid URL.');
  }

  if (!['http:', 'https:', 'ws:', 'wss:'].includes(url.protocol)) {
    throw new Error('Gateway must use http, https, ws, or wss.');
  }
  if (!url.hostname || url.username || url.password || url.search || url.hash) {
    throw new Error('Gateway address cannot include credentials, query parameters, or fragments.');
  }
  if (url.pathname !== '/' && url.pathname !== '') {
    throw new Error('Gateway address must not include a path.');
  }

  const secure = url.protocol === 'https:' || url.protocol === 'wss:';
  const httpProtocol = secure ? 'https:' : 'http:';
  const wsProtocol = secure ? 'wss:' : 'ws:';
  const authority = `${url.hostname.includes(':') ? `[${url.hostname}]` : url.hostname}${url.port ? `:${url.port}` : ''}`;
  return { httpBase: `${httpProtocol}//${authority}`, wsUrl: `${wsProtocol}//${authority}/ws` };
}

/** Query auth is needed because React Native's WebSocket cannot set Authorization. */
export function authenticatedWebSocketUrl(wsUrl: string, token: string): string {
  const url = new URL(wsUrl);
  url.searchParams.set('token', token);
  return url.toString();
}
