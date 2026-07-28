import { clientReducer, initialClientState } from '../src/protocol/reducer';

describe('clientReducer', () => {
  it('hydrates session lists and history', () => {
    const sessions = clientReducer(initialClientState, { type: 'wire', frame: { type: 'session_list', sessions: [{ session_id: 's1', title: 'Work' }] } });
    const history = clientReducer(sessions, { type: 'wire', frame: { type: 'history', session_id: 's1', messages: [{ id: 'm1', role: 'user', content: 'hello' }] } });
    expect(history.sessions[0]?.title).toBe('Work');
    expect(history.activeSessionId).toBe('s1');
    expect(history.transcript).toEqual([{ id: 'm1', kind: 'user', text: 'hello' }]);
  });

  it('streams replacement text and tool lifecycle', () => {
    let state = clientReducer(initialClientState, { type: 'wire', frame: { type: 'text_delta', text: 'hel' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'text_delta', text: 'lo' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'text_replace', text: 'hello!' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'tool_start', id: 'tool-1', name: 'bash' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'tool_exec', id: 'tool-1', name: 'bash' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'tool_done', id: 'tool-1', name: 'bash', output: 'ok' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'message_end' } });
    expect(state.transcript[0]?.text).toBe('hello!');
    expect(state.transcript[1]).toMatchObject({ toolName: 'bash', toolState: 'done', text: 'ok' });
    expect(state.isStreaming).toBe(false);
  });

  it('captures session ID, swarm state, errors, and done frames', () => {
    let state = clientReducer(initialClientState, { type: 'wire', frame: { type: 'session_id', session_id: 's2' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'swarm_status', status: '3 running' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'error', message: 'nope' } });
    state = clientReducer(state, { type: 'wire', frame: { type: 'done' } });
    expect(state).toMatchObject({ activeSessionId: 's2', swarmStatus: '3 running', error: 'nope', isStreaming: false });
  });
});
