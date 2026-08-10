import { needsHistoryAfterSubscribe, normalizeSessionWorkingDir, sessionSubscribeRequest } from '../src/protocol/session';

describe('session gateway requests', () => {
  it('includes the absolute working directory required by Subscribe', () => {
    expect(sessionSubscribeRequest('session-1', ' /workspaces/jcode ')).toEqual({
      type: 'subscribe',
      target_session_id: 'session-1',
      working_dir: '/workspaces/jcode',
    });
  });

  it('uses a safe absolute fallback for deep links without session metadata', () => {
    expect(normalizeSessionWorkingDir('relative/path')).toBe('/');
    expect(sessionSubscribeRequest('session-1')).toMatchObject({ working_dir: '/' });
  });

  it('does not request history twice when resume already returns it', () => {
    expect(needsHistoryAfterSubscribe({ type: 'history' })).toBe(false);
    expect(needsHistoryAfterSubscribe({ type: 'done', id: 1 })).toBe(true);
  });
});
