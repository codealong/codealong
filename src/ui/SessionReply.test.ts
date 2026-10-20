import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Question, Session } from '../types';
import { boxPlace, givenBack, replyPlaceholder, takesReply } from './SessionReply';

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

  it('says a session stuck mid-turn takes it once that step ends', () => {
    expect(replyPlaceholder(session({ status: 'needs', stalledSince: 1 }), 'Ghostty', { kind: 'background' })).toBe('Send to claude via its background job: it takes this once the step it’s stuck on ends');
    // One whose turn ended on a command left in the background sits at its prompt.
    expect(replyPlaceholder(session({ status: 'needs', stalledSince: 1, background: 'acme login --device' }), 'Ghostty', { kind: 'typed', app: 'Ghostty' })).toBe('Send to claude via Ghostty');
  });

  it('keeps its own words for a session Codealong runs', () => {
    expect(replyPlaceholder(session({ hosted: true, status: 'working' }), 'Ghostty', { kind: 'typed', app: 'Ghostty' })).toBe('Send to claude: it takes this after its current turn');
  });
});

describe('boxPlace', () => {
  it('says where a session runs by how a message gets there, else by where it is', () => {
    const said = (p: { icon: string; label: string }) => `${p.icon} ${p.label}`;
    expect(said(boxPlace(session(), 'Ghostty', { kind: 'typed', app: 'iTerm2' }))).toBe('terminal iTerm2');
    expect(said(boxPlace(session(), 'Ghostty', { kind: 'background' }))).toBe('stack Background');
    expect(said(boxPlace(session(), 'Ghostty', { kind: 'codex-app' }))).toBe('app Codex app');
    expect(said(boxPlace(session({ hosted: true }), 'Ghostty', { kind: 'typed', app: 'Ghostty' }))).toBe('app Codealong');
    // Before the engine has said, by where it is.
    expect(said(boxPlace(session(), 'Ghostty', null))).toBe('terminal Ghostty');
    expect(boxPlace(session(), 'Ghostty', { kind: 'background' }).title).toBe('It runs in the background');
  });

  it('says where a session that isn’t running goes on: the place picked for it', () => {
    const stopped = session({ live: false });
    expect(boxPlace(stopped, 'Ghostty', { kind: 'resume', app: 'Ghostty' }, { place: 'background', label: 'Background' })).toEqual({ icon: 'stack', label: 'Background', title: 'It isn’t running: a send goes on with it in the background' });
    expect(boxPlace(stopped, 'Ghostty', { kind: 'resume', app: 'Ghostty' }).label).toBe('Ghostty');
  });
});

describe('takesReply', () => {
  it('takes a message from anything but a session at a prompt', () => {
    expect(takesReply(session({ status: 'working' }))).toBe(true);
    expect(takesReply(session({ status: 'needs' }))).toBe(false);
    expect(takesReply(session({ status: 'needs', asks: true }))).toBe(true);
    // Stuck mid-turn, or on a command its turn left in the background, it takes one as its own
    // terminal does.
    expect(takesReply(session({ status: 'needs', stalledSince: 1 }))).toBe(true);
    expect(takesReply(session({ status: 'needs', stalledSince: 1, background: 'acme login --device' }))).toBe(true);
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
