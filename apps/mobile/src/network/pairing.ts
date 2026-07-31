import { normalizeGatewayUrl } from './urls';
import type { Credential, PairRequest, PairResponse } from '../protocol/types';

export type FetchLike = typeof fetch;

function errorMessage(body: unknown, fallback: string): string {
  if (body && typeof body === 'object' && typeof (body as { error?: unknown }).error === 'string') {
    return (body as { error: string }).error;
  }
  return fallback;
}

export async function pairGateway(
  gateway: string,
  request: PairRequest,
  fetchImpl: FetchLike = fetch,
): Promise<Credential> {
  const { httpBase } = normalizeGatewayUrl(gateway);
  const response = await fetchImpl(`${httpBase}/pair`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: JSON.stringify(request),
  });
  const body: unknown = await response.json().catch(() => undefined);
  if (!response.ok) throw new Error(errorMessage(body, `Pairing failed (${response.status}).`));
  const pair = body as Partial<PairResponse>;
  if (!pair.token || !pair.server_name || !pair.server_version) {
    throw new Error('Pairing response did not include a valid device token.');
  }
  return { gateway: httpBase, token: pair.token, server_name: pair.server_name, server_version: pair.server_version };
}
