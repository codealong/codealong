// What the bar says of voice (ui/Said.tsx): a key held to talk, as it's heard and where it went
// (src-tauri/src/dictation.rs `Talk`), and a listening session's latest cue (listen.rs
// `Glance::cue`), the thing a coach wants you to see now. One shows at a time, under the notch's
// pills or in the floating bar, for as long as it's of use and no longer: the notch is over
// whatever you're doing.

import { cuesYou } from './listening';
import type { BarListening, Talk } from './types';

/** What the bar shows of voice now. */
export type Said = { kind: 'talk'; talk: Talk } | { kind: 'cue'; listening: BarListening };

/** How long a key held to talk stays up once it's over, by how it ended: long enough to read
 *  where it went, longer for what went wrong. None while it's still going. */
export function lingers(phase: Talk['phase']): number | null {
  switch (phase) {
    case 'listening':
    case 'settling':
      return null;
    case 'failed':
      return 8000;
    case 'nothing':
    case 'asked':
      return 4000;
    default:
      return 2500;
  }
}

/** The key held to talk to show, when the engine says something of one: the one it's about,
 *  unless a later press is showing already. An earlier press's words settle after the next has
 *  begun, and what they came to isn't news over what's being said now. */
export function latestTalk(shown: Talk | null, told: Talk): Talk {
  return shown && told.id < shown.id ? shown : told;
}

/** How long a cue stays up: three short lines, read at a glance from a meeting. */
export const CUE_SHOWS = 12_000;

/** What shows now: the key held to talk while it's going and for a moment after (`talkedAt` is
 *  when its last word came), else the newest cue that's fresh and wasn't put away (`dismissed`
 *  is the time of the last one that was). */
export function saidNow(talk: Talk | null, talkedAt: number, listenings: readonly BarListening[], dismissed: number, now: number): Said | null {
  if (talk) {
    const stays = lingers(talk.phase);
    if (stays === null || now - talkedAt < stays) return { kind: 'talk', talk };
  }
  const fresh = listenings.filter((l) => l.cue && cuesYou(l.role) && l.cueAt > dismissed && now - l.cueAt < CUE_SHOWS).sort((a, b) => b.cueAt - a.cueAt)[0];
  return fresh ? { kind: 'cue', listening: fresh } : null;
}

/** When what shows now stops showing by itself, for a timer to draw again then; null when it
 *  stays until something changes. */
export function saidUntil(said: Said | null, talkedAt: number): number | null {
  if (!said) return null;
  if (said.kind === 'cue') return said.listening.cueAt + CUE_SHOWS;
  const stays = lingers(said.talk.phase);
  return stays === null ? null : talkedAt + stays;
}

/** What the mark says while sessions hear (ui/Said.tsx, the bar's mark with its center red):
 *  which, and what's wrong with any, a line each. One started ahead of its session, which is
 *  still on its way, is heard under its meeting's name. Nothing while none does. */
export function hearingSays(listenings: readonly BarListening[] | undefined): string | null {
  if (!listenings?.length) return null;
  return listenings.map((l) => `${l.title} is ${l.starting !== undefined ? 'being heard, and its session is starting' : 'listening'}${l.problem ? `: ${l.problem}` : ''}`).join('\n');
}

/** The sessions hearing now in a few words, where there's room for one name: the one's title, or
 *  how many there are (the main window's bar, ui/Listening.tsx `ListeningNow`). */
export function hearingNamed(listenings: readonly BarListening[]): string {
  return listenings.length === 1 ? listenings[0].title : `${listenings.length} sessions listening`;
}

/** How much of what's heard a line at a window's foot holds: the end of it, which is what's new. */
const FOOT_LINE = 80;

/** A key held to talk in one line, for a window that stands in for the bar where there's none
 *  (talk.ts `hear`): what it is (`talkSays`), and after it the end of what's heard so far while
 *  it's going, or what there is to know of how it ended. */
export function heardLine(talk: Talk): string {
  const going = talk.phase === 'listening' || talk.phase === 'settling';
  const more = (going ? talk.text : '') || talk.message || '';
  const words = more.length > FOOT_LINE ? `…${more.slice(-FOOT_LINE)}` : more;
  return words ? `${talkSays(talk)}: ${words}` : talkSays(talk);
}

/** What a key held to talk says of itself, over the words: who's listening, and where they went. */
export function talkSays(talk: Talk): string {
  const whom = talk.title ?? (talk.to === 'orchestrator' ? 'your orchestrator' : null);
  switch (talk.phase) {
    case 'listening':
      return talk.to === 'orchestrator' ? 'Listening, for your orchestrator' : 'Listening';
    case 'settling':
      return 'Got it';
    case 'typed':
      return whom ? `In ${whom}’s box` : 'Typed';
    case 'sent':
      return whom ? `Sent to ${whom}` : 'Sent';
    case 'asked':
      return 'In the quick panel';
    case 'started':
      return 'Starting a session with it';
    case 'nothing':
      return 'Nothing heard';
    case 'failed':
      return 'Couldn’t listen';
  }
}
