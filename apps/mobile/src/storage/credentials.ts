import * as SecureStore from 'expo-secure-store';
import type { Credential } from '../protocol/types';

const CREDENTIAL_KEY = 'jcode.mobile.credential.v1';
const DEVICE_ID_KEY = 'jcode.mobile.device-id.v1';

export async function loadCredential(): Promise<Credential | undefined> {
  const stored = await SecureStore.getItemAsync(CREDENTIAL_KEY);
  if (!stored) return undefined;
  try {
    const value: unknown = JSON.parse(stored);
    if (!value || typeof value !== 'object') return undefined;
    const candidate = value as Partial<Credential>;
    return candidate.gateway && candidate.token && candidate.server_name && candidate.server_version
      ? candidate as Credential : undefined;
  } catch {
    return undefined;
  }
}

export async function saveCredential(credential: Credential): Promise<void> {
  await SecureStore.setItemAsync(CREDENTIAL_KEY, JSON.stringify(credential));
}

export async function clearCredential(): Promise<void> {
  await SecureStore.deleteItemAsync(CREDENTIAL_KEY);
}

export async function deviceId(): Promise<string> {
  const existing = await SecureStore.getItemAsync(DEVICE_ID_KEY);
  if (existing) return existing;
  const fresh = globalThis.crypto?.randomUUID?.() ?? `expo-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  await SecureStore.setItemAsync(DEVICE_ID_KEY, fresh);
  return fresh;
}
