import { afterEach, describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import { setPlatform } from '../platform';
import type { Question, Session } from '../types';
import { bangSays, bangTitle, boxPlace, givenBack, replyPlaceholder, stillOffered, takesReply, typesCommands } from './SessionReply';

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
    expect(replyPlaceholder(session({ status: 'needs', stalledSince: 1, need: 'stuck' }), 'Ghostty', { kind: 'background' })).toBe('Send to claude via its background job: it takes this once the step it’s stuck on ends');
    // One whose turn ended on a command left in the background sits at its prompt.
    expect(replyPlaceholder(session({ status: 'needs', stalledSince: 1, need: 'stuck', background: 'acme login --device' }), 'Ghostty', { kind: 'typed', app: 'Ghostty' })).toBe('Send to claude via Ghostty');
  });

  it('keeps its own words for a session Codealong runs', () => {
    expect(replyPlaceholder(session({ hosted: true, status: 'working' }), 'Ghostty', { kind: 'typed', app: 'Ghostty' })).toBe('Send to claude: it takes this after its current turn');
  });
});

describe('typesCommands', () => {
  it('takes a harness’s command where what’s sent lands in the session’s box: its terminal or a background job’s', () => {
    expect(typesCommands({ kind: 'typed', app: 'Ghostty' })).toBe(true);
    expect(typesCommands({ kind: 'background' })).toBe(true);
    // A message hands the agent the words, and an app or a resume isn't the running session's box.
    expect(typesCommands({ kind: 'message' })).toBe(false);
    expect(typesCommands({ kind: 'codex-app' })).toBe(false);
    expect(typesCommands({ kind: 'resume', app: 'Ghostty' })).toBe(false);
    expect(typesCommands(null)).toBe(false);
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

describe('bangSays', () => {
  afterEach(() => setPlatform(null));

  it('says ⏎ runs a command led by ! in a terminal of yours, and ⌥⏎ in the session’s own shell where its harness takes it', () => {
    setPlatform('mac');
    expect(bangSays(session(), 'Ghostty', true, { kind: 'typed', app: 'Ghostty' })).toBe('⏎ runs it in a terminal under the session · ⌥⏎ in claude’s own shell');
    expect(bangSays(session({ harness: 'codex', account: 'codex' }), 'Ghostty', false, { kind: 'typed', app: 'Ghostty' })).toBe('⏎ runs it in a new Ghostty tab · ⌥⏎ in codex’s own shell');
    // A harness that can't take one from here, or none said yet: ⏎ alone, with what follows.
    expect(bangSays(session({ harness: 'cursor', account: 'cursor' }), 'Ghostty', true, null)).toBe('⏎ runs it in a terminal under the session, as you; cursor is told what it prints once it ends.');
    // The button's title says the rest.
    expect(bangTitle(session(), 'Ghostty', true, { kind: 'background' })).toBe('Run it in a terminal under the session, as you (⏎): claude is told what it prints once it ends. ⌥-click or ⌥⏎ runs it in claude’s own shell instead, as Claude Code’s ! does');
    expect(bangTitle(session({ harness: 'cursor', account: 'cursor' }), 'Ghostty', false, null)).toBe('Run it in a new Ghostty tab, as you (⏎): cursor is told what it prints once it ends');
  });

  it('says the keys as Linux has them', () => {
    setPlatform('linux');
    expect(bangSays(session(), 'Ghostty', true, { kind: 'background' })).toBe('Enter runs it in a terminal under the session · Alt+Enter in claude’s own shell');
  });
});

describe('takesReply', () => {
  it('takes a message from anything but a session at a prompt', () => {
    expect(takesReply(session({ status: 'working' }))).toBe(true);
    expect(takesReply(session({ status: 'needs' }))).toBe(false);
    expect(takesReply(session({ status: 'needs', asks: true, need: 'reply' }))).toBe(true);
    // Stuck mid-turn, or on a command its turn left in the background, it takes one as its own
    // terminal does.
    expect(takesReply(session({ status: 'needs', stalledSince: 1, need: 'stuck' }))).toBe(true);
    expect(takesReply(session({ status: 'needs', stalledSince: 1, need: 'stuck', background: 'acme login --device' }))).toBe(true);
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

describe('stillOffered', () => {
  const color: Question = { question: 'Which color?', multiSelect: false, options: [{ label: 'Red' }, { label: 'Blue' }] };
  const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }] };

  it('keeps the picks its questions still offer', () => {
    expect(stillOffered([color, days], { 'Which color?': ['Red'], 'Which days?': ['Mon', 'Tue'] })).toEqual({ 'Which color?': ['Red'], 'Which days?': ['Mon', 'Tue'] });
    // Other choices now: the question is untouched again, so what was suggested shows.
    expect(stillOffered([{ ...color, options: [{ label: 'Green' }, { label: 'Yellow' }] }], { 'Which color?': ['Red'] })).toEqual({});
    // One of them gone.
    expect(stillOffered([{ ...days, options: [{ label: 'Tue' }, { label: 'Wed' }] }], { 'Which days?': ['Mon', 'Tue'] })).toEqual({ 'Which days?': ['Tue'] });
    // Taking one now, it keeps the first.
    expect(stillOffered([{ ...days, multiSelect: false }], { 'Which days?': ['Tue', 'Mon'] })).toEqual({ 'Which days?': ['Tue'] });
    // A question no longer asked, and one whose picks were all taken back.
    expect(stillOffered([days], { 'Which color?': ['Red'], 'Which days?': [] })).toEqual({ 'Which days?': [] });
  });
});
