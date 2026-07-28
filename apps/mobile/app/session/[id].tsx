import { useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { Button, KeyboardAvoidingView, Platform, SafeAreaView, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { useMobile } from '../../src/ui/mobile-context';

export default function SessionTranscript() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { selectSession, sendMessage, state } = useMobile();
  const [draft, setDraft] = useState('');

  useEffect(() => { if (id) selectSession(id); }, [id, selectSession]);
  const submit = () => { const text = draft.trim(); if (!text) return; setDraft(''); sendMessage(text); };

  return <KeyboardAvoidingView style={styles.root} behavior={Platform.select({ ios: 'padding', default: undefined })}>
    <SafeAreaView style={styles.root}>
      {state.error ? <Text style={styles.error}>{state.error}</Text> : null}
      <ScrollView contentContainerStyle={styles.transcript} keyboardShouldPersistTaps="handled">
        {state.transcript.length === 0 ? <Text style={styles.empty}>Loading transcript…</Text> : state.transcript.map((entry) => (
          <View key={entry.id} style={[styles.entry, entry.kind === 'user' ? styles.user : entry.kind === 'tool' ? styles.tool : styles.assistant]}>
            {entry.kind === 'tool' ? <Text style={styles.toolLabel}>{entry.toolName} · {entry.toolState}</Text> : null}
            <Text style={styles.entryText}>{entry.text}</Text>
          </View>
        ))}
        {state.isStreaming ? <Text style={styles.streaming}>Jcode is working…</Text> : null}
      </ScrollView>
      <View style={styles.composer}><TextInput style={styles.input} value={draft} onChangeText={setDraft} placeholder="Message Jcode" placeholderTextColor="#7f899c" multiline accessibilityLabel="Message composer" /><Button title="Send" onPress={submit} disabled={!draft.trim() || state.connection !== 'connected'} /></View>
    </SafeAreaView>
  </KeyboardAvoidingView>;
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#10131a' }, transcript: { gap: 10, padding: 16 }, empty: { color: '#aeb8ca', textAlign: 'center', marginTop: 30 }, entry: { borderRadius: 12, padding: 12, maxWidth: '92%' }, user: { backgroundColor: '#1f4f86', alignSelf: 'flex-end' }, assistant: { backgroundColor: '#1a202b', alignSelf: 'flex-start' }, tool: { backgroundColor: '#292638', alignSelf: 'flex-start' }, entryText: { color: '#f5f7fa', fontSize: 16, lineHeight: 23 }, toolLabel: { color: '#d8c78a', fontWeight: '700', fontSize: 12, marginBottom: 5 }, streaming: { color: '#aeb8ca', marginVertical: 4 }, error: { color: '#ff9ca2', backgroundColor: '#3a1e27', padding: 10 }, composer: { flexDirection: 'row', alignItems: 'flex-end', gap: 8, padding: 12, borderTopWidth: 1, borderTopColor: '#2d3748' }, input: { flex: 1, maxHeight: 120, backgroundColor: '#1a202b', borderWidth: 1, borderColor: '#2d3748', borderRadius: 12, padding: 12, color: '#f5f7fa', fontSize: 16 },
});
