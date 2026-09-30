import { describe, expect, it } from 'vitest';
import type { Session } from '../types';
import { replyPlaceholder } from './SessionReply';

const session = (over: Partial<Session> = {}): Session => ({ account: 'claude', harness: 'claude', surface: 'cli', status: 'idle', live: true, asks: false, hosted: false, ...over }) as Session;

describe('replyPlaceholder', () => {
  it('says how a message gets there once the engine has said', () => {
    expect(replyPlaceholder(session(), 'Ghostty', { kind: 'typed', app: 'iTerm2' })).toBe('Send to claude via iTerm2');
    expect(replyPlaceholder(session({ status: 'working' }), 'Ghostty', { kind: 'background' })).toBe('Send to claude via its background job: it takes this in as it works');
    expect(replyPlaceholder(session({ surface: 'app' }), 'Ghostty', { kind: 'message' })).toBe('Send to claude as a message from Codealong');
    expect(replyPlaceholder(session({ asks: true, status: 'needs' }), 'Ghostty', { kind: 'typed', app: 'tmux' })).toBe('Answer claude via tmux');
    expect(replyPlaceholder(session({ live: false }), 'Ghostty', { kind: 'resume', app: 'Terminal' })).toBe('Send to claude: it isn’t running, so this resumes it in Terminal');
  });

  it('goes by the session alone until then', () => {
    expect(replyPlaceholder(session({ status: 'working' }), 'Ghostty')).toBe('Send to claude: it takes this in as it works');
    expect(replyPlaceholder(session({ live: false }), 'Ghostty', null)).toBe('Send to claude: it isn’t running, so this resumes it in Ghostty');
  });

  it('keeps its own words for a session Codealong runs', () => {
    expect(replyPlaceholder(session({ hosted: true, status: 'working' }), 'Ghostty', { kind: 'typed', app: 'Ghostty' })).toBe('Send to claude: it takes this after its current turn');
  });
});
