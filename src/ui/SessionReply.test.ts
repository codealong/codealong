import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Question, Session } from '../types';
import { givenBack, replyPlaceholder } from './SessionReply';

const session = (over: Partial<Session> = {}): Session => fx.session({ live: true, asks: false, hosted: false, ...over });

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

describe('givenBack', () => {
  const color: Question = { question: 'Which color?', multiSelect: false, options: [{ label: 'Red' }, { label: 'Blue' }] };
  const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }] };

  it('has the box pick again what an answer that didn’t go picked, with its words', () => {
    expect(givenBack([color, days], { 'Which color?': 'Green', 'Which days?': ['Mon', 'only mornings'] })).toEqual({
      picks: { 'Which color?': [], 'Which days?': ['Mon'] },
      own: { 'Which color?': 'Green', 'Which days?': 'only mornings' },
    });
    expect(givenBack([color], { 'Which color?': 'Blue' })).toEqual({ picks: { 'Which color?': ['Blue'] }, own: {} });
    // Words of your own that name a choice stay words: the answer is sent again as it was.
    expect(givenBack([days], { 'Which days?': ['Tue', 'Mon, only mornings'] })).toEqual({ picks: { 'Which days?': ['Tue'] }, own: { 'Which days?': 'Mon, only mornings' } });
    expect(givenBack([color, days], undefined)).toEqual({ picks: {}, own: {} });
  });
});
