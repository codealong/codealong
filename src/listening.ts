// A session that listens, as the windows know it (src-tauri/src/listen.rs): which sessions are
// hearing a meeting now and whether voice is turned on, kept here from every board a window
// follows (followBoard.ts), so a session's menu, its row's chip and its panel all read it from one
// place; and how what Codealong tells a listening session reads back, so its conversation is
// drawn as the meeting it heard with what its agent made of it in between (ui/Listening.tsx).
//
// What's told starts in words listen.rs writes (`HEARD`, `news`, `brief`), which are matched
// here: the two change together, and the tests on both sides hold the same lines.

import { useSyncExternalStore } from 'react';
import { homeOf } from './lanes';
import type { Codec } from './stored';
import type { BarListening, Board, Hearing, HearingStatus, Lane, ListenAsk, ListeningGlance, ListenRole, MeetingAhead, Status } from './types';

/** Voice as the last board had it: whether it's turned on, the key held to talk to what you're
 *  looking at (as config.toml writes it, empty when off), and the sessions hearing now. */
export interface Voice {
  on: boolean;
  talkKeys: string;
  listenings: Readonly<Record<string, ListeningGlance>>;
}

const NONE: Voice = { on: false, talkKeys: '', listenings: {} };
let voice: Voice = NONE;
const watchers = new Set<() => void>();

/** Takes what a board says of voice. The same as before changes nothing, so nothing draws again. */
export function noteVoice(board: Pick<Board, 'experimental' | 'listenings' | 'talkShortcut'>): void {
  const next: Voice = { on: !!board.experimental?.voice, talkKeys: board.talkShortcut ?? '', listenings: board.listenings ?? NONE.listenings };
  if (next.on === voice.on && next.talkKeys === voice.talkKeys && JSON.stringify(next.listenings) === JSON.stringify(voice.listenings)) return;
  voice = next;
  for (const w of watchers) w();
}

export function voiceNow(): Voice {
  return voice;
}

const watch = (w: () => void) => {
  watchers.add(w);
  return () => {
    watchers.delete(w);
  };
};

export function useVoice(): Voice {
  return useSyncExternalStore(watch, voiceNow);
}

/** Whether a session that's hearing (`Session.listening`) shows that as its status, in place of
 *  its own glyph (ui/primitives.tsx `SessionGlyph`). Its turns come and go every few seconds while it listens,
 *  none of them yours (board.rs `apply_listening`), so working and idle say little of it; that it
 *  waits on you, or that something went wrong, still shows. */
export function hearingShows(status: Status): boolean {
  return status !== 'needs' && status !== 'error';
}

/** The sessions hearing now with where each is, the first to begin first, as the bar's feed names
 *  them (barfeed.rs `listenings`): what the main window's recording light shows. One whose
 *  session the board doesn't list is left out, but one started ahead of its session
 *  (`ListeningGlance.starting`), which hears under its meeting's name with no session to show yet. */
export function hearingNow(lanes: Lane[], listenings: Voice['listenings'] | undefined): BarListening[] {
  const out: BarListening[] = [];
  for (const [key, l] of Object.entries(listenings ?? {})) {
    if (l.starting !== undefined) {
      out.push({ ...l, key: '', laneId: '', title: l.starting });
      continue;
    }
    const home = homeOf(lanes, key);
    if (home) out.push({ ...l, key, laneId: home.lane.id, title: home.session.title });
  }
  return out.sort((a, b) => a.startedAt - b.startedAt || a.key.localeCompare(b.key));
}

/** A session's listening while it's hearing, as the board has it. */
export function useListening(key: string): ListeningGlance | undefined {
  return useVoice().listenings[key];
}

/** What a listening session can be asked to make of what it hears, in the dialog's order. */
export const ROLES: { id: ListenRole; label: string; says: string }[] = [
  { id: 'notes', label: 'Notes', says: 'Keeps notes as the meeting goes: what’s decided, who took on what, questions left open. Sums it up when it ends.' },
  { id: 'coach', label: 'Coach', says: 'Tells you, a few words at a time, what to ask or come back to. Its latest shows in the notch, by your camera.' },
  { id: 'requests', label: 'Requests', says: 'Files a todo for each thing asked for on a call: a bug, a fix, a feature, with what was said about it.' },
  { id: 'custom', label: 'Your own', says: 'Does what you write below with what it hears.' },
];

/** What a new listening asks for until you say otherwise: you and everyone else, for notes. */
export const ASK: ListenAsk = { mic: true, system: true, apps: [], role: 'notes', prompt: '', start: false };

/** Why a listening can't be asked for as it stands, in the engine's words (listen.rs
 *  `Ask::sources`), or null when it can. */
export function askProblem(ask: ListenAsk): string | null {
  if (ask.role === 'custom' && !ask.prompt.trim()) return 'Say what the session should do with what it hears.';
  if (!ask.mic && !ask.system) return 'Pick something to listen to: the microphone, this Mac’s audio, or both.';
  return null;
}

/** What Listen now remembers of how you last asked (the quick panel set to listen, ui/Quick.tsx):
 *  what its session makes of what it hears, and what's heard. What you type there is the
 *  meeting's own, and never kept. */
export interface ListenKept {
  role: ListenRole;
  mic: boolean;
  system: boolean;
  start: boolean;
}

export const LISTEN_KEPT: ListenKept = { role: ASK.role, mic: ASK.mic, system: ASK.system, start: ASK.start };

/** `ListenKept` as a window keeps it. What it can't read of one it reads as a new one's. */
export const listenKept: Codec<ListenKept> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
    const kept = v as Partial<Record<keyof ListenKept, unknown>>;
    const role = ROLES.find((r) => r.id === kept.role)?.id ?? LISTEN_KEPT.role;
    const on = (x: unknown, otherwise: boolean) => (typeof x === 'boolean' ? x : otherwise);
    return { role, mic: on(kept.mic, LISTEN_KEPT.mic), system: on(kept.system, LISTEN_KEPT.system), start: on(kept.start, LISTEN_KEPT.start) };
  },
  format: (kept) => JSON.stringify(kept),
};

/** What Listen now asks for, from how you last asked and what you typed this time: the meeting's
 *  name and anything you want of it, or for Your own, what to do with what's heard. */
export function nowAsk(kept: ListenKept, typed: string): ListenAsk {
  return { ...ASK, ...kept, start: kept.start && kept.role === 'requests', prompt: typed.trim() };
}

/** The meeting in your calendar that Listen now would be of: the one it was opened for
 *  (`asked`, picked in your meetings), whatever its time, until a session hears it, as the
 *  latest read of the day ahead has it or as it was picked before that read is in; else one
 *  that's going on or about to start and that no session hears yet (meetings.rs `Ahead`), the
 *  first to start. None while meetings aren't offered, since the calendar isn't read then. */
export function nowMeeting(ahead: readonly MeetingAhead[], asked?: MeetingAhead | null): MeetingAhead | null {
  const picked = asked ? (ahead.find((m) => m.id === asked.id) ?? asked) : null;
  return picked && !picked.heard ? picked : (ahead.find((m) => m.takeable && !m.heard) ?? null);
}

/** What the session Listen now starts does with what it hears, as the end of a sentence. */
const NOW_DOES: Record<ListenRole, string> = {
  notes: 'keeps its notes',
  coach: 'coaches you through it',
  requests: 'files a todo for each thing asked for',
  custom: 'does what you wrote with it',
};

/** What the box asks for when the quick panel is set to listen. */
export function nowPlaceholder(role: ListenRole): string {
  return role === 'custom' ? 'What should it do with what it hears?' : 'What’s this meeting? You can leave this empty';
}

/** The row that starts a listening, in words: what ⏎ does, and what follows. `where` is where its
 *  session starts ("as a chat of its own", "in web-2"). */
export function nowRow(c: { kept: ListenKept; meeting: Pick<MeetingAhead, 'title'> | null; where: string }): { title: string; why: string } {
  const title = c.meeting ? `Listen to ${c.meeting.title.trim() ? `“${c.meeting.title.trim()}”` : 'the meeting in your calendar'}` : 'Listen now';
  const from = c.kept.mic && c.kept.system ? 'you and this Mac’s audio' : c.kept.mic ? 'your microphone' : 'this Mac’s audio';
  return { title, why: `Codealong hears ${from} from the moment you press ⏎, and a session started ${c.where} ${NOW_DOES[c.kept.role]}. No audio is kept.` };
}

/** What the dialog says, before a listening starts, of what macOS has to allow for what's picked
 *  (ear.rs `Status`): that it isn't letting Codealong, with where that's turned on, or that it
 *  will ask. Nothing when all of it is allowed, or isn't known. */
export function macSays(status: HearingStatus['status'] | undefined, ask: Pick<ListenAsk, 'mic' | 'system' | 'apps'>): string {
  if (!status) return '';
  const mic = ask.mic ? status.microphone : 'allowed';
  const plays = ask.system || ask.apps.length > 0 ? (status.systemAudio ?? '') : 'allowed';
  if (mic === 'denied' || mic === 'restricted') return 'macOS isn’t letting Codealong use the microphone: turn Codealong on in System Settings › Privacy & Security › Microphone.';
  if (plays === 'denied') return 'macOS isn’t letting Codealong hear what this Mac plays: turn Codealong on in System Settings › Privacy & Security › Screen & System Audio Recording, under System Audio Recording Only.';
  const asks = [...(mic === 'unasked' ? ['use the microphone'] : []), ...(plays === 'unasked' ? ['hear what this Mac plays'] : [])];
  return asks.length ? `macOS will ask whether Codealong may ${asks.join(', and whether it may ')}.` : '';
}

/** What a listening hears, in a few words: "You and this Mac’s audio". */
export function hears(l: Pick<ListeningGlance, 'mic' | 'playback' | 'apps' | 'speakers'>, names: Record<string, string> = {}): string {
  const them = l.apps?.length ? l.apps.map((a) => names[a] ?? a).join(', ') : 'this Mac’s audio';
  // On speakers, as it was heard to be: the microphone hears the others too.
  if (l.mic && l.playback) return `You and ${them}${l.speakers ? ', on speakers' : ''}`;
  return l.mic ? 'Your microphone' : them.charAt(0).toUpperCase() + them.slice(1);
}

/** How long something has gone on, as a clock: "4:07", "1:02:45". */
export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const two = (n: number) => String(n).padStart(2, '0');
  const [h, m] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60)];
  return h > 0 ? `${h}:${two(m)}:${two(s % 60)}` : `${m}:${two(s % 60)}`;
}

/** One line of a transcript (listen.rs `line`, `aside`): something said, or with no `who`
 *  something Codealong says of the transcript itself. */
export interface HeardLine {
  at: string;
  who: string | null;
  text: string;
}

export function heardLine(line: string): HeardLine {
  const said = /^\[(\d\d:\d\d:\d\d)\] ([^:(][^:]*): (.*)$/.exec(line);
  if (said) return { at: said[1], who: said[2], text: said[3] };
  const aside = /^\[(\d\d:\d\d:\d\d)\] \((.*)\)$/.exec(line);
  if (aside) return { at: aside[1], who: null, text: aside[2] };
  return { at: '', who: null, text: line };
}

/** A line of a transcript with its number, counted from 1 (listen.rs `Listening.lines`). */
export interface NumberedLine {
  n: number;
  said: string;
}

/** What a listening's strip shows as it's heard (ui/Listening.tsx `LiveStrip`), as captions: the
 *  latest lines heard, told to its session or not, and the words each speaker is saying now. You
 *  and the others can talk at once, and each settles by itself, so their words are kept apart by
 *  who says them, in the order each began. `settled` is when a speaker's words were heard
 *  settling: they stay until the line they settled into comes, which the engine sends just after,
 *  so the strip doesn't lose a row for a moment and take it back. */
export interface Captions {
  lines: readonly NumberedLine[];
  saying: ReadonlyMap<string, { text: string; settled: number | null }>;
}

export const NO_CAPTIONS: Captions = { lines: [], saying: new Map() };

/** How many lines a strip keeps: more than its window shows, so the window is full once a meeting
 *  has gone on a little. The rest are in the conversation, or on their way to it. */
export const CAPTIONS_KEPT = 12;

/** How long settled words wait for their line before they go anyway. */
export const SETTLING_MS = 1500;

/** The lines a strip shows once `more` come in, `first` the number of the first of them (which
 *  says where they start when there are none): each once, in the transcript's order, the latest
 *  `CAPTIONS_KEPT` of the last run of lines one after another. A strip shown again starts from
 *  what it showed before (ui/Listening.tsx), and lines its session was told while it wasn't
 *  shown aren't among what it reads then: the run before them goes, rather than read on into
 *  the lines after the gap, or stand for the latest, as though nothing was said since. */
export function withLines(c: Captions, more: readonly NumberedLine[], first = more[0]?.n): Captions {
  const last = c.lines[c.lines.length - 1];
  const before = last && first !== undefined && last.n < first - 1 ? [] : c.lines;
  if (more.length === 0) return before === c.lines ? c : { ...c, lines: before };
  const byN = new Map(before.map((l) => [l.n, l]));
  for (const l of more) byN.set(l.n, l);
  const all = [...byN.values()].sort((a, b) => a.n - b.n);
  let from = all.length - 1;
  while (from > 0 && all[from - 1].n === all[from].n - 1) from--;
  return { ...c, lines: all.slice(Math.max(from, all.length - CAPTIONS_KEPT)) };
}

/** Captions once `h` is heard, at `at`. How far the session has been told changes nothing: the
 *  strip shows what's heard, whether or not the conversation above has it yet. */
export function captionsHearing(c: Captions, h: Pick<Hearing, 'who' | 'text' | 'line' | 'said'>, at: number): Captions {
  if (h.line !== undefined && h.said !== undefined) {
    const next = withLines(c, [{ n: h.line, said: h.said }]);
    // The line takes the place of the words it settled from.
    const who = heardLine(h.said).who;
    return who !== null && next.saying.has(who) ? { ...next, saying: without(next.saying, who) } : next;
  }
  if (h.who === undefined) return c;
  const was = c.saying.get(h.who);
  if (!h.text) return was ? { ...c, saying: new Map(c.saying).set(h.who, { text: was.text, settled: at }) } : c;
  return { ...c, saying: new Map(c.saying).set(h.who, { text: h.text, settled: null }) };
}

/** Captions once `who`'s words that settled at `at` have waited `SETTLING_MS` for their line: gone,
 *  unless they've been taken up again since. */
export function settledGone(c: Captions, who: string, at: number): Captions {
  return c.saying.get(who)?.settled === at ? { ...c, saying: without(c.saying, who) } : c;
}

function without<V>(m: ReadonlyMap<string, V>, key: string): Map<string, V> {
  const next = new Map(m);
  next.delete(key);
  return next;
}

/** What Codealong told a listening session, read back from its conversation. */
export type Told =
  /** What was heard: lines `from` to `to` of the transcript at `path`, with the lines themselves
   *  unless there were too many to carry; `ended` when it's the last of it, with why. */
  | { kind: 'heard'; from: number; to: number; path: string; lines: HeardLine[] | null; ended: string | null }
  /** That it ended with nothing more said, and why. */
  | { kind: 'ended'; why: string }
  /** What's going on, as its listening began: `words` is all of it. */
  | { kind: 'began'; words: string };

const HEARD = /^Heard, lines? (\d+)(?: to (\d+))? of (.+?)(?::\n|, too much to put here: read it there\.)/;
const ENDED = /(Your user ended the listening\.|The listening has ended: .+?\.) (?:Nothing more was said\. )?Finish up as you were asked to\.$/;
const BEGAN = 'Your user asked you to listen in on a meeting through Codealong.';

/** Why a listening ended, from the sentence that says it. */
function whyEnded(sentence: string): string {
  const why = /^The listening has ended: (.+)\.$/.exec(sentence);
  return why ? `It ended: ${why[1]}.` : 'You ended it.';
}

/** `text`, a message Codealong sent a session by itself, as what it told a listening session;
 *  null for anything else Codealong sent. */
export function toldIn(text: string): Told | null {
  const t = text.trim();
  if (t.startsWith(BEGAN)) return { kind: 'began', words: t };
  const ended = ENDED.exec(t);
  const heard = HEARD.exec(t);
  if (!heard) return ended && t.startsWith(ended[1]) ? { kind: 'ended', why: whyEnded(ended[1]) } : null;
  const from = Number(heard[1]);
  const quoted = t.split('\n').filter((l) => l.startsWith('> '));
  return {
    kind: 'heard',
    from,
    to: heard[2] ? Number(heard[2]) : from,
    path: heard[3],
    lines: quoted.length > 0 ? quoted.map((l) => heardLine(l.slice(2))) : null,
    ended: ended ? whyEnded(ended[1]) : null,
  };
}

/** A reply that says there's nothing to say (listen.rs `says_nothing`): a dash, as a listening
 *  session is asked to answer when it has nothing for you. */
export function saysNothing(reply: string): boolean {
  const r = reply
    .trim()
    .replace(/^[`*.]+|[`*.]+$/g, '')
    .trim();
  return r === '' || r === '-' || r === '–' || r === '—' || r === '--';
}

/** A listening session's short answer with each of its lines a paragraph of its own: it's asked
 *  for a few short lines to take in at a glance, which Markdown would run together into one. A
 *  longer answer (the summary at the end) is left as written. */
export function asCue(reply: string): string {
  const lines = reply.trim().split('\n');
  if (lines.length < 2 || lines.length > 6 || lines.some((l) => l.trim() === '' || /^\s*([-*+#>|]|\d+\.|```)/.test(l))) return reply;
  return lines.map((l) => l.trim()).join('\n\n');
}

/** Whether a role's cues are for now, and worth opening the notch on: a coach's, a call's
 *  requests as they're filed, your own. Notes say only that they were kept. */
export function cuesYou(role: ListenRole): boolean {
  return role !== 'notes';
}

const ASKED = 'codealong-listen';

/** What "Listen…" in a session's menu asks for: the dialog, for this session. */
export interface ListenAsked {
  key: string;
  title: string;
  /** Its worktree, for the main window to show the session with the dialog. */
  laneId: string;
}

/** Asks this window for the dialog that sets a session's listening up (ui/Listening.tsx
 *  `ListenHost`, which every window that has session menus mounts). */
export function askToListen(asked: ListenAsked): void {
  window.dispatchEvent(new CustomEvent<ListenAsked>(ASKED, { detail: asked }));
}

export function onAskedToListen(cb: (asked: ListenAsked) => void): () => void {
  const heard = (e: Event) => cb((e as CustomEvent<ListenAsked>).detail);
  window.addEventListener(ASKED, heard);
  return () => window.removeEventListener(ASKED, heard);
}
