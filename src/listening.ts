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
import type { BarListening, Board, Hearing, HearingStatus, Lane, ListenAsk, ListeningGlance, ListenRole, Status } from './types';

/** Voice as the last board had it: whether it's turned on, and the sessions hearing now. */
export interface Voice {
  on: boolean;
  listenings: Readonly<Record<string, ListeningGlance>>;
}

const NONE: Voice = { on: false, listenings: {} };
let voice: Voice = NONE;
const watchers = new Set<() => void>();

/** Takes what a board says of voice. The same as before changes nothing, so nothing draws again. */
export function noteVoice(board: Pick<Board, 'experimental' | 'listenings'>): void {
  const next: Voice = { on: !!board.experimental?.voice, listenings: board.listenings ?? NONE.listenings };
  if (next.on === voice.on && JSON.stringify(next.listenings) === JSON.stringify(voice.listenings)) return;
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
 *  session the board doesn't list is left out. */
export function hearingNow(lanes: Lane[], listenings: Voice['listenings'] | undefined): BarListening[] {
  const out: BarListening[] = [];
  for (const [key, l] of Object.entries(listenings ?? {})) {
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
export const ASK: ListenAsk = { mic: true, system: true, apps: [], speakers: false, role: 'notes', prompt: '', start: false };

/** Why a listening can't be asked for as it stands, in the engine's words (listen.rs
 *  `Ask::sources`), or null when it can. */
export function askProblem(ask: ListenAsk): string | null {
  if (ask.role === 'custom' && !ask.prompt.trim()) return 'Say what the session should do with what it hears.';
  if (!ask.mic && !ask.system) return 'Pick something to listen to: the microphone, this Mac’s audio, or both.';
  return null;
}

/** What a listening hears, in a few words: "You and this Mac’s audio". */
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

export function hears(l: Pick<ListeningGlance, 'mic' | 'playback' | 'apps' | 'speakers'>, names: Record<string, string> = {}): string {
  const them = l.apps?.length ? l.apps.map((a) => names[a] ?? a).join(', ') : 'this Mac’s audio';
  // On speakers, as it was heard to be: the Mac's audio is being taken out of the microphone.
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

/** The lines a strip shows once `more` come in: each once, in the transcript's order, the latest
 *  `CAPTIONS_KEPT`. */
export function withLines(c: Captions, more: readonly NumberedLine[]): Captions {
  if (more.length === 0) return c;
  const byN = new Map(c.lines.map((l) => [l.n, l]));
  for (const l of more) byN.set(l.n, l);
  return { ...c, lines: [...byN.values()].sort((a, b) => a.n - b.n).slice(-CAPTIONS_KEPT) };
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
