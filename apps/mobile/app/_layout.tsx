import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { MobileProvider } from '../src/ui/mobile-context';

export default function RootLayout() {
  return (
    <MobileProvider>
      <StatusBar style="light" />
      <Stack screenOptions={{ headerStyle: { backgroundColor: '#10131a' }, headerTintColor: '#f5f7fa', contentStyle: { backgroundColor: '#10131a' } }}>
        <Stack.Screen name="index" options={{ title: 'Jcode' }} />
        <Stack.Screen name="session/[id]" options={{ title: 'Session' }} />
      </Stack>
    </MobileProvider>
  );
}
