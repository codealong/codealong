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
// Nothing here sends anything: what a machine heard is yours to read over first.

import { onTalk, onTalked, showQuick } from './api';
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

/** Has this window put what's said where its caret is, and say what's heard meanwhile when the
 *  engine tells it (there's no bar to), for as long as it's open. */
export function typeWhatsSaid(): void {
  onTalk(hear);
  onTalked(place);
}
