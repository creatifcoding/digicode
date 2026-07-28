import { createContext, useCallback, useContext, useEffect, useReducer, useRef, useState } from 'react';
import { pairGateway } from '../network/pairing';
import { GatewayTransport } from '../network/transport';
import { normalizeGatewayUrl } from '../network/urls';
import { clientReducer, initialClientState } from '../protocol/reducer';
import type { ClientState, Credential } from '../protocol/types';
import { clearCredential, deviceId, loadCredential, saveCredential } from '../storage/credentials';

type MobileContextValue = {
  ready: boolean;
  credential?: Credential;
  state: ClientState;
  pair: (gateway: string, code: string, deviceName: string) => Promise<void>;
  selectSession: (sessionId: string) => void;
  sendMessage: (content: string) => void;
  refreshSessions: () => void;
  forgetDevice: () => Promise<void>;
};

const MobileContext = createContext<MobileContextValue | undefined>(undefined);

export function MobileProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(clientReducer, initialClientState);
  const [credential, setCredential] = useState<Credential>();
  const [ready, setReady] = useState(false);
  const transportRef = useRef<GatewayTransport | undefined>(undefined);

  const establish = useCallback((nextCredential: Credential) => {
    transportRef.current?.stop();
    const transport = new GatewayTransport({
      ...normalizeGatewayUrl(nextCredential.gateway),
      token: nextCredential.token,
      onFrame: (frame) => dispatch({ type: 'wire', frame }),
      onState: (connection, attempt, error) => dispatch({ type: 'connection', state: connection, attempt, error }),
      onOpen: () => {
        // A correlated request refreshes session state after every reconnect.
        void transport.request({ type: 'list_sessions' }).catch(() => undefined);
      },
    });
    transportRef.current = transport;
    transport.start();
  }, []);

  useEffect(() => {
    let mounted = true;
    void loadCredential().then((stored) => {
      if (!mounted) return;
      setCredential(stored);
      if (stored) establish(stored);
      setReady(true);
    });
    return () => {
      mounted = false;
      transportRef.current?.stop();
    };
  }, [establish]);

  const pair = useCallback(async (gateway: string, code: string, deviceName: string) => {
    const credential = await pairGateway(gateway, {
      code: code.trim(),
      device_id: await deviceId(),
      device_name: deviceName.trim() || 'Jcode Mobile',
    });
    await saveCredential(credential);
    setCredential(credential);
    establish(credential);
  }, [establish]);

  const selectSession = useCallback((sessionId: string) => {
    dispatch({ type: 'select_session', sessionId });
    const transport = transportRef.current;
    if (!transport) return;
    void (async () => {
      try {
      // subscribe carries target_session_id for existing Jcode sessions.
        await transport.request({ type: 'subscribe', target_session_id: sessionId });
        // Request history only after the subscribe `done` acknowledgement.
        await transport.request({ type: 'get_history' });
      } catch {
        dispatch({ type: 'wire', frame: { type: 'error', message: 'Could not attach to this session.' } });
      }
    })();
  }, []);

  const sendMessage = useCallback((content: string) => {
    const text = content.trim();
    if (!text) return;
    const transport = transportRef.current;
    if (!transport) {
      dispatch({ type: 'wire', frame: { type: 'error', message: 'Gateway is not connected.' } });
      return;
    }
    dispatch({ type: 'optimistic_message', content: text });
    try {
      transport.send({ type: 'message', content: text });
    } catch {
      dispatch({ type: 'wire', frame: { type: 'error', message: 'Message could not be sent. It was not queued.' } });
    }
  }, []);

  const refreshSessions = useCallback(() => {
    const transport = transportRef.current;
    if (!transport) return;
    void transport.request({ type: 'list_sessions' }).catch(() => undefined);
  }, []);

  const forgetDevice = useCallback(async () => {
    transportRef.current?.stop();
    transportRef.current = undefined;
    await clearCredential();
    setCredential(undefined);
    dispatch({ type: 'reset' });
  }, []);

  return <MobileContext.Provider value={{ ready, credential, state, pair, selectSession, sendMessage, refreshSessions, forgetDevice }}>{children}</MobileContext.Provider>;
}

export function useMobile(): MobileContextValue {
  const context = useContext(MobileContext);
  if (!context) throw new Error('useMobile must be used inside MobileProvider.');
  return context;
}
