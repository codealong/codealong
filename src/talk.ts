// What's said with a key held to talk (src-tauri/src/dictation.rs), put where you'd have typed
// it. The engine hands it to the Codealong window that had the keyboard as the key went down
// (the `talked` event), and that window puts it, in this order:
//
// - where its caret is, when a text field has the keyboard: a session's box, a todo's
//   description, a review. It goes in as typing does, so the field's own code hears it and Undo
//   takes it out again;
// - into the box of the session the window shows, when nothing has the keyboard; or of the
//   session the engine names (your orchestrator's), whatever the window shows;
// - into the quick panel, opened on it, when the window shows no session: it finds the session
//   the words are for, or starts one.
//
// The quick panel takes what's said into its own box itself (ui/Quick.tsx), which shows the
// words there as they're heard, and uses `place` for its other fields.
//
// What's heard shows on the bar as it goes. Where there's no bar to show it (it's in the menu
// bar, or off), the engine tells the window the words are for instead (the `talk` event), and
// that window says it in a line at its foot (`hear`), so that it's listening and what it has
// heard are in sight all the same. The line is only said: nothing the words will go into is
// touched until they're settled.
//
// The window the words are for is told of its press whatever the bar does, and keeps the one
// going (`useGoing`), with the field that had the keyboard as it began: a session's box shows its
// microphone listening by it (ui/SessionReply.tsx `TalkButton`), the press begun by the key held
// or by that microphone.
//
// Nothing here sends anything: what a machine heard is yours to read over first.

import { useSyncExternalStore } from 'react';
import { onTalk, onTalked, showQuick, talkTo } from './api';
import { addSpoken, saidAfter } from './drafts';
import { heardLine, lingers } from './said';
import type { Talk, Talked } from './types';

/** The text field that has the keyboard here, if one does. */
function fieldWithKeys(): HTMLTextAreaElement | HTMLInputElement | null {
  const el = document.activeElement;
  if (el instanceof HTMLTextAreaElement) return el.readOnly || el.disabled ? null : el;
  if (el instanceof HTMLInputElement) return ['text', 'search', ''].includes(el.type) && !el.readOnly && !el.disabled ? el : null;
  return null;
}

/** Types `text` at the caret of `field`, as the keyboard would: the page's own handlers hear it
 *  and Undo knows of it. False where the page wouldn't take it. */
function typeAt(field: HTMLTextAreaElement | HTMLInputElement, text: string): boolean {
  const at = field.selectionStart ?? field.value.length;
  const words = saidAfter(field.value.slice(0, at), text);
  // The one way to put text in a field that its framework hears as typing, and Undo with it.
  return document.execCommand('insertText', false, words);
}

/** Puts what was said where it's for in this window. */
export function place(talked: Talked): void {
  // Not said when it fails: with the quick panel gone too there's nowhere left to say it.
  const ask = () => void showQuick({ prompt: talked.text }).catch(() => {});
  if (talked.key) return addSpoken(talked.key, talked.text);
  const field = fieldWithKeys();
  if (field && typeAt(field, talked.text)) return;
  const shown = document.querySelector<HTMLElement>('[data-shows-session]')?.dataset.showsSession;
  if (shown) return addSpoken(shown, talked.text);
  ask();
}

/** The line at this window's foot while it stands in for the bar, the press it's of, and what
 *  takes it away once that press is over. */
let line: HTMLElement | null = null;
let lineOf = 0;
let lineGoes = 0;

/** Says what's heard of a press at the foot of this window (`heardLine`): there while the key is
 *  down, and for a moment after, as long as the bar would say where it went (`lingers`). An
 *  earlier press's words settle after the next has begun, and say nothing over it. */
export function hear(talk: Talk): void {
  if (talk.id < lineOf) return;
  lineOf = talk.id;
  window.clearTimeout(lineGoes);
  if (!line?.isConnected) {
    line = document.createElement('div');
    line.className = 'ca-talking';
    line.setAttribute('role', 'status');
    document.body.appendChild(line);
  }
  line.dataset.phase = talk.phase;
  line.textContent = heardLine(talk);
  const stays = lingers(talk.phase);
  if (stays !== null) lineGoes = window.setTimeout(() => line?.remove(), stays);
}

/** A press whose words are for a field of this window's, while it's heard (listening, then
 *  settling), and the field that had the keyboard as it began, where its words go. None for a
 *  press to your orchestrator, whose words go to its session wherever you are, nor one begun with
 *  no field to type into. */
export interface Going {
  talk: Talk;
  field: Element;
}

let going: Going | null = null;
/** The latest press this window was told of: an earlier one settling after it says nothing. */
let latest = 0;
const watchers = new Set<() => void>();

/** Notes what this window is told of a press, for `useGoing`. */
function noteGoing(talk: Talk): void {
  if (talk.id < latest) return;
  const field = talk.id !== latest ? (talk.to === 'looking' ? fieldWithKeys() : null) : (going?.field ?? null);
  latest = talk.id;
  const next = field && (talk.phase === 'listening' || talk.phase === 'settling') ? { talk, field } : null;
  if (!next && !going) return;
  going = next;
  for (const w of watchers) w();
}

/** The press a field's microphone began last (`talkInto`): its id once the engine has said it,
 *  and whether the field went first (`dropBegunIn`). */
interface Begun {
  field: Element;
  id: number | null;
  gone: boolean;
}

let begun: Begun | null = null;

/** Begins talking into `field` with its microphone, as the key held to talk going down does
 *  (dictation.rs): the field gets the keyboard, where the words go. A field gone before the
 *  engine says which press it began has that press dropped as soon as it does. */
export async function talkInto(field: HTMLElement): Promise<void> {
  field.focus();
  const b: Begun = { field, id: null, gone: false };
  begun = b;
  b.id = await talkTo(true);
  if (b.gone && b.id !== null) await talkTo(false, b.id);
}

/** Ends the press heard into `field`, with the keyboard given back to it first, so its words are
 *  typed at its caret however the press was stopped (a keyboard user's Space on the microphone
 *  leaves the keyboard there). */
export async function talkedInto(field: HTMLElement): Promise<void> {
  field.focus();
  await talkTo(false);
}

/** Drops the press `field`'s microphone began, if it may still be going, as the field goes (its
 *  peek put away, another session shown): no key held will end it, and its words would land in
 *  whatever has the keyboard next. The engine drops it too as its window closes or hides
 *  (dictation.rs `window_gone`). A press begun by the key ends as the key comes up, as ever. */
export function dropBegunIn(field: Element): void {
  const b = begun;
  if (!b || b.field !== field) return;
  begun = null;
  b.gone = true;
  // Its id not said yet: `talkInto` drops it once it is. Over already (told of, and not the
  // one going): nothing to drop.
  if (b.id === null || (b.id <= latest && going?.talk.id !== b.id)) return;
  // Not said when it fails: the box is gone, and a press ends by itself after three minutes.
  void talkTo(false, b.id).catch(() => {});
}

const watchGoing = (w: () => void) => {
  watchers.add(w);
  return () => {
    watchers.delete(w);
  };
};

/** The press going in this window, if one is. */
export function goingNow(): Going | null {
  return going;
}

/** `goingNow`, drawn again as it changes. */
export function useGoing(): Going | null {
  return useSyncExternalStore(watchGoing, goingNow);
}

let typesHere = false;

/** Whether this window types what's said: the bar's doesn't, so a box there has no microphone. */
export function talksHere(): boolean {
  return typesHere;
}

/** Has this window put what's said where its caret is, keep the press going (`useGoing`), and say
 *  what's heard meanwhile where the engine says there's no bar to, for as long as it's open. */
export function typeWhatsSaid(): void {
  typesHere = true;
  onTalk((talk) => {
    noteGoing(talk);
    if (talk.standsIn) hear(talk);
  });
  onTalked(place);
}
