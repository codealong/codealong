// What's said with a key held to talk (src-tauri/src/dictation.rs), put where you'd have typed
// it. The engine hands it to the Codealong window that had the keyboard as the key went down
// (the `talked` event), and that window puts it, in this order:
//
// - where its caret is, when a text field has the keyboard: a session's box, the quick panel, a
//   todo's description, a review. It goes in as typing does, so the field's own code hears it and
//   Undo takes it out again;
// - into the box of the session the window shows, when nothing has the keyboard; or of the
//   session the engine names (your orchestrator's), whatever the window shows;
// - into the quick panel, opened on it, when the window shows no session: it finds the session
//   the words are for, or starts one.
//
// And it shows there as it's heard (the `talk` event, which the engine sends this window too):
// the words so far sit at the caret of the field they're for, selected, since they aren't
// settled yet and what's typed as the key comes up takes their place. So what's being heard is
// in sight where it's going, whatever the bar is set to show, and with no bar at all.
//
// Nothing here sends anything: what a machine heard is yours to read over first.

import { onTalk, onTalked, showQuick } from './api';
import { addSpoken, saidAfter } from './drafts';
import type { Talk, Talked } from './types';

type Field = HTMLTextAreaElement | HTMLInputElement;

/** The text field that has the keyboard here, if one does. */
function fieldWithKeys(): Field | null {
  const el = document.activeElement;
  if (el instanceof HTMLTextAreaElement) return el.readOnly || el.disabled ? null : el;
  if (el instanceof HTMLInputElement) return ['text', 'search', ''].includes(el.type) && !el.readOnly && !el.disabled ? el : null;
  return null;
}

/** Types `text` at the caret of `field`, as the keyboard would: the page's own handlers hear it
 *  and Undo knows of it. False where the page wouldn't take it. */
function typeAt(field: Field, text: string): boolean {
  const at = field.selectionStart ?? field.value.length;
  const words = saidAfter(field.value.slice(0, at), text);
  // The one way to put text in a field that its framework hears as typing, and Undo with it.
  return document.execCommand('insertText', false, words);
}

/** What's being heard of a press, where this window shows it: the field, and where in it the
 *  words so far are (nothing of it until the first of them). */
interface Hearing {
  id: number;
  field: Field;
  start: number;
  end: number;
  shown: boolean;
}

let hearing: Hearing | null = null;

/** Puts `words` in place of what's at `from` to `to` in a field, as its framework hears a change
 *  (not as typing: Undo knows nothing of words that weren't settled), selected or the caret
 *  after them. */
function put(field: Field, words: string, from: number, to: number, select: boolean): void {
  field.setRangeText(words, from, to, select ? 'select' : 'end');
  field.dispatchEvent(new Event('input', { bubbles: true }));
}

/** Takes what was shown of a press out of its field again: nothing came of it there. */
function unshow(): void {
  const h = hearing;
  hearing = null;
  if (h?.shown && h.field.isConnected) put(h.field, '', h.start, h.end, false);
}

/** Shows what's heard of a press as it goes (`talk`): the words so far at the caret of the field
 *  that has the keyboard, selected. Only what's said to whatever you're looking at: what's for
 *  your orchestrator goes to its box, wherever that is. Once the press is over, what was shown
 *  has been typed over (`place`), or goes. */
export function hear(talk: Talk): void {
  if (talk.to !== 'looking') return;
  if (hearing && hearing.id !== talk.id) unshow();
  if (talk.phase !== 'listening' && talk.phase !== 'settling') return unshow();
  const field = hearing?.field ?? fieldWithKeys();
  if (!field?.isConnected) return;
  hearing ??= { id: talk.id, field, start: field.selectionStart ?? field.value.length, end: field.selectionEnd ?? field.value.length, shown: false };
  const words = saidAfter(field.value.slice(0, hearing.start), talk.text);
  // Nothing heard yet leaves the field as it is, what's selected in it included.
  if (!hearing.shown && !words) return;
  if (hearing.shown && field.value.slice(hearing.start, hearing.end) === words) return;
  put(field, words, hearing.start, hearing.end, true);
  hearing.end = hearing.start + words.length;
  hearing.shown = true;
}

/** Puts what was said where it's for in this window. */
export function place(talked: Talked): void {
  // Not said when it fails: with the quick panel gone too there's nowhere left to say it.
  const ask = () => void showQuick({ prompt: talked.text }).catch(() => {});
  const field = fieldWithKeys();
  // What was shown as it was heard gives way to what was said: typed over it where it's in the
  // field that takes it (one Undo takes the lot out), and taken out anywhere else.
  const over = hearing?.shown && hearing.field === field && !talked.key ? hearing : null;
  if (over && field) {
    field.setSelectionRange(over.start, over.end);
    hearing = null;
  } else unshow();
  if (talked.key) return addSpoken(talked.key, talked.text);
  if (field && typeAt(field, talked.text)) return;
  // The field wouldn't take it as typing: what was shown in it goes, and the words go on to
  // where they can be put.
  if (over && field) put(field, '', over.start, over.end, false);
  const shown = document.querySelector<HTMLElement>('[data-shows-session]')?.dataset.showsSession;
  if (shown) return addSpoken(shown, talked.text);
  ask();
}

/** Has this window show what's said where its caret is as it's heard, and put it there once it's
 *  said, for as long as it's open. */
export function typeWhatsSaid(): void {
  onTalk(hear);
  onTalked(place);
}
