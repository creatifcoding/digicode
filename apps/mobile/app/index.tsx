import { router } from 'expo-router';
import { useState } from 'react';
import { ActivityIndicator, Button, FlatList, SafeAreaView, StyleSheet, Text, TextInput, View } from 'react-native';
import { useMobile } from '../src/ui/mobile-context';

function ConnectionBadge() {
  const { state } = useMobile();
  const label = state.connection === 'reconnecting' ? `Reconnecting (${state.reconnectAttempt})` : state.connection;
  return <View style={[styles.badge, state.connection === 'connected' ? styles.connected : styles.disconnected]}><Text style={styles.badgeText}>{label}</Text></View>;
}

function Pairing() {
  const { pair } = useMobile();
  const [gateway, setGateway] = useState('');
  const [code, setCode] = useState('');
  const [deviceName, setDeviceName] = useState('Jcode Mobile');
  const [error, setError] = useState<string>();
  const [submitting, setSubmitting] = useState(false);
  const submit = async () => {
    setSubmitting(true); setError(undefined);
    try { await pair(gateway, code, deviceName); } catch (caught) { setError(caught instanceof Error ? caught.message : 'Pairing failed.'); }
    finally { setSubmitting(false); }
  };
  return <SafeAreaView style={styles.screen}><View style={styles.card}>
    <Text style={styles.eyebrow}>SECURE DEVICE PAIRING</Text><Text style={styles.title}>Connect to Jcode</Text>
    <Text style={styles.help}>Run jcode pair on your host, then enter its reachable LAN or Tailscale address and one-time code.</Text>
    <TextInput style={styles.input} value={gateway} onChangeText={setGateway} autoCapitalize="none" autoCorrect={false} placeholder="http://jcode-host:7643" placeholderTextColor="#7f899c" accessibilityLabel="Gateway address" />
    <TextInput style={styles.input} value={code} onChangeText={setCode} autoCapitalize="characters" autoCorrect={false} placeholder="Pairing code" placeholderTextColor="#7f899c" accessibilityLabel="Pairing code" />
    <TextInput style={styles.input} value={deviceName} onChangeText={setDeviceName} placeholder="Device name" placeholderTextColor="#7f899c" accessibilityLabel="Device name" />
    {error ? <Text style={styles.error}>{error}</Text> : null}
    <Button title={submitting ? 'Pairing…' : 'Pair device'} disabled={submitting || !gateway || !code} onPress={() => void submit()} />
    <Text style={styles.finePrint}>Your device token is stored in the platform secure store. It is never written to application logs.</Text>
  </View></SafeAreaView>;
}

function Sessions() {
  const { credential, forgetDevice, refreshSessions, state } = useMobile();
  return <SafeAreaView style={styles.screen}><View style={styles.header}>
    <View><Text style={styles.eyebrow}>{credential?.server_name?.toUpperCase()}</Text><Text style={styles.title}>Sessions</Text></View><ConnectionBadge />
  </View>
  {state.error ? <Text style={styles.error}>{state.error}</Text> : null}
  {state.swarmStatus ? <Text style={styles.notice}>Swarm: {state.swarmStatus}</Text> : null}
  <FlatList data={state.sessions} keyExtractor={(item) => item.session_id} contentContainerStyle={state.sessions.length ? styles.list : styles.emptyList}
    ListEmptyComponent={<View><Text style={styles.emptyTitle}>No sessions received</Text><Text style={styles.help}>Reconnect or refresh after the gateway reports session_list.</Text></View>}
    renderItem={({ item }) => <View style={styles.session}><View style={styles.sessionText}><Text style={styles.sessionTitle} numberOfLines={1}>{item.title || item.session_id}</Text><Text style={styles.sessionMeta} numberOfLines={1}>{item.status || 'idle'} · {item.working_dir || 'unknown workspace'}</Text></View><Button title="Open" onPress={() => router.push({ pathname: '/session/[id]', params: { id: item.session_id, working_dir: item.working_dir || '/' } })} /></View>}
  />
  <View style={styles.footer}><Button title="Refresh" onPress={refreshSessions} /><Button title="Forget device" color="#dc6b72" onPress={() => void forgetDevice()} /></View>
  </SafeAreaView>;
}

export default function Home() {
  const { ready, credential } = useMobile();
  if (!ready) return <View style={styles.loading}><ActivityIndicator color="#7ab6ff" /></View>;
  return credential ? <Sessions /> : <Pairing />;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#10131a', padding: 20 }, loading: { flex: 1, backgroundColor: '#10131a', justifyContent: 'center' }, card: { gap: 14, marginTop: 48 }, eyebrow: { color: '#7ab6ff', fontWeight: '700', fontSize: 12, letterSpacing: 1.2 }, title: { color: '#f5f7fa', fontSize: 30, fontWeight: '700', marginTop: 4 }, help: { color: '#aeb8ca', fontSize: 15, lineHeight: 22 }, finePrint: { color: '#7f899c', fontSize: 12, lineHeight: 18 }, input: { backgroundColor: '#1a202b', borderColor: '#2d3748', borderWidth: 1, borderRadius: 10, padding: 13, color: '#f5f7fa', fontSize: 16 }, error: { color: '#ff9ca2', backgroundColor: '#3a1e27', padding: 10, borderRadius: 8 }, notice: { color: '#d8c78a', marginBottom: 10 }, header: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 18 }, badge: { paddingHorizontal: 10, paddingVertical: 6, borderRadius: 999 }, connected: { backgroundColor: '#173827' }, disconnected: { backgroundColor: '#34252a' }, badgeText: { color: '#e4eaf3', fontWeight: '600', fontSize: 12 }, list: { gap: 10 }, emptyList: { flexGrow: 1, justifyContent: 'center', alignItems: 'center' }, emptyTitle: { color: '#f5f7fa', fontSize: 18, fontWeight: '600', marginBottom: 8 }, session: { flexDirection: 'row', alignItems: 'center', gap: 10, backgroundColor: '#181d27', borderRadius: 12, padding: 14 }, sessionText: { flex: 1, gap: 5 }, sessionTitle: { color: '#f5f7fa', fontSize: 16, fontWeight: '600' }, sessionMeta: { color: '#aeb8ca', fontSize: 13 }, footer: { flexDirection: 'row', justifyContent: 'space-between', paddingVertical: 14 },
});
