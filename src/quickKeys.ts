// The quick panel's keys (ui/Quick.tsx): the box's own (`onQuickBoxKey`), and the panel's wherever
// the keyboard is in it but a dialog (`onQuickKey`). Each is a function of the key and a record of
// what the panel has as the key comes: what it reads, and what it can be asked to do. The panel
// passes the record as its latest render has it. ui/Quick.test.ts presses every key on the page.
//
// ⏎ does the selected row and ⌘⏎ always starts; ⌥⏎ does a row's other thing; ⇥ answers a session
// or reviews a PR in the waiting window, and finishes a skill's name. ↑ in the empty box walks
// back through the draft and earlier prompts. ⌘1 to ⌘9 do a row, ⌘P picks the project and ⌥⌘1 to
// ⌥⌘9 switch to one, ⌘D the machine, ⌘T the repo, ⌘B the worktree, ⌘L the agent and ⌘O where it
// runs; ⌘F is Send to and back, ⇧⌘A lists archived sessions in a search, ⇧⌘D takes a calmer
// machine, and Escape, ⌘Q and ⌘H put the panel away.

import { useEffect, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { hideQuick, talkTo } from './api';
import { CALMER_CHORD, type Calmer } from './machineLoad';
import { bare, chordPressed, cmd, emacsCtrl, held as modsHeld } from './platform';
import type { Row } from './quickRows';
import { errorText } from './report';
import type { SkillRow } from './skills';
import { withAttachments } from './todos';
import type { Attachment, Lane, ProjectSummary, Session } from './types';
import { dialogUp } from './ui/focusTrap';
import { projectForDigit } from './ui/ProjectSwitcher';

/** The panel's menus, by what each picks. */
export type PickerKind = 'project' | 'device' | 'repo' | 'worktree' | 'agent';

/** What the box's keys read and do, as the panel has it. */
export interface QuickBoxKeys {
  /** A press is being heard into the box (dictation.rs): what it shows isn't what it holds yet. */
  hearingHere: object | null;
  /** The panel is set to listen: the box holds what the meeting is. */
  listen: boolean;
  /** The box finds the session a prompt goes to. */
  inSendTo: boolean;
  /** The rows listed, and every row's key in the order the arrows go through them, the new
   *  session's last; the one selected. */
  rows: Row[];
  keys: string[];
  active: string;
  /** The draft and the prompts before it that ↑ walks back through, where the walk is (null
   *  while it isn't on one), and what's attached to what the box holds. */
  recallable: string[];
  recall: number | null;
  attached: Attachment[];
  setError: (message: string) => void;
  listenToIt: () => void;
  activate: (key: string, alt: boolean) => void;
  closeSendTo: () => void;
  move: (delta: number) => void;
  pickSkill: (row: SkillRow) => void;
  answerSession: (lane: Lane, session: Session) => void;
  answerIn: (key: string) => void;
  start: () => void;
  /** Brings back the prompt at `at` of `recallable` into the box, or the empty box at -1. */
  recallAt: (at: number, el: HTMLTextAreaElement) => void;
}

/** Does what a key pressed in the panel's box does. */
export function onQuickBoxKey(e: ReactKeyboardEvent<HTMLTextAreaElement>, box: QuickBoxKeys): void {
  const { hearingHere, listen, inSendTo, rows, keys, active, recallable, recall, attached, setError, listenToIt, activate, closeSendTo, move, pickSkill, answerSession, answerIn, start, recallAt } = box;
  if (e.nativeEvent.isComposing) return;
  // While a press is heard into the box, what it shows isn't what it holds yet, so nothing is
  // done with it. ⏎ ends a press begun with the microphone: what was said is typed, to read.
  if (hearingHere) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void talkTo(false).catch((err: unknown) => setError(errorText(err)));
    }
    return;
  }
  // Set to listen there's nothing listed to move through, and no earlier prompts to bring
  // back: ⏎ starts listening, and ⇧⏎ is a new line.
  if (listen) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      listenToIt();
    }
    return;
  }
  const el = e.currentTarget;
  // In Send to the box finds a session: ⏎ (or ⌘⏎) sends to it, ⌫ in the empty box goes back to
  // the prompt as Escape does, and the arrows move through the list, never back through
  // earlier prompts. What's typed there is one line.
  if (inSendTo && e.key !== 'Tab') {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (!e.shiftKey) activate(active, e.altKey);
    } else if (e.key === 'Backspace' && !el.value && bare(e)) {
      e.preventDefault();
      closeSendTo();
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !e.altKey && !cmd(e) && !e.shiftKey) {
      e.preventDefault();
      move(e.key === 'ArrowDown' ? 1 : -1);
    } else if (!cmd(e) && (emacsCtrl(e, 'n') || emacsCtrl(e, 'p'))) {
      e.preventDefault();
      move(e.key === 'n' ? 1 : -1);
    }
    return;
  }
  // ⇥ on a session answers it in the waiting window; elsewhere it moves the focus as usual.
  if (e.key === 'Tab' && bare(e)) {
    const row = rows.find((r) => r.key === active);
    if (row?.kind === 'skill') {
      e.preventDefault();
      pickSkill(row);
      return;
    }
    const at = row?.kind === 'session' ? row : row?.kind === 'suggest' && (row.s.kind === 'send' || row.s.kind === 'open') ? row.s : null;
    if (at) {
      e.preventDefault();
      answerSession(at.lane, at.session);
      return;
    }
    // And on a PR waiting on your review, it reviews it there, as ⏎ does.
    if (row?.kind === 'review') {
      e.preventDefault();
      answerIn(row.pr.key);
      return;
    }
  }
  if (e.key === 'Enter') {
    if (e.shiftKey) return;
    e.preventDefault();
    if (cmd(e)) start();
    else activate(active, e.altKey);
    return;
  }
  // ↑ in the empty input brings back the draft, then earlier prompts, and ↓ goes forward again
  // to the empty input; once you change what came back, the arrows move through the list.
  const recalled = recall !== null && withAttachments(el.value, attached) === recallable[recall];
  const plainKey = bare(e);
  const walk = (e.key === 'ArrowUp' && (recalled || (!el.value && attached.length === 0 && keys.indexOf(active) <= 0))) || (e.key === 'ArrowDown' && recalled);
  if (plainKey && walk) {
    const at = recalled ? recall + (e.key === 'ArrowUp' ? 1 : -1) : 0;
    if (at >= recallable.length) return e.preventDefault();
    e.preventDefault();
    recallAt(at, el);
    return;
  }
  if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !e.altKey && !cmd(e) && !e.shiftKey) {
    // In a prompt of several lines the arrows move the cursor, and the selection only from
    // its first or last line.
    const before = el.value.slice(0, el.selectionStart);
    const after = el.value.slice(el.selectionEnd);
    if (e.key === 'ArrowUp' && before.includes('\n')) return;
    if (e.key === 'ArrowDown' && after.includes('\n')) return;
    e.preventDefault();
    move(e.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (!cmd(e) && (emacsCtrl(e, 'n') || emacsCtrl(e, 'p'))) {
    e.preventDefault();
    move(e.key === 'n' ? 1 : -1);
  }
}

/** What the panel's keys read and do, as the panel has it. */
export interface QuickKeys {
  /** Switch to Codealong Dev? is asking (DevSetupDialog.tsx): it has the keys. */
  askingDev: boolean;
  /** Folded to the chip that holds your prompt. */
  folded: boolean;
  /** The menu that's open. */
  picker: PickerKind | null;
  inSendTo: boolean;
  hearingHere: object | null;
  listen: boolean;
  projects: ProjectSummary[];
  /** The rows ⌘1 to ⌘9 reach, in order, and the row selected. */
  numbered: Row[];
  active: string;
  /** What's looked for, and how many sessions what's typed could be sent to. */
  query: string;
  sendable: number;
  /** A calm machine offered while the start's is strained. */
  calmer: Calmer | null;
  /** The repo a session starts in, whose worktrees ⌘B lists; your other machines; and the places
   *  the account starts in. */
  repoNow: object | null;
  macs: readonly unknown[];
  places: readonly unknown[];
  /** The agent menu's ← → step where it runs, not the effort. */
  stepsPlace: boolean;
  setPicker: (kind: PickerKind | null) => void;
  togglePicker: (kind: PickerKind) => void;
  setStepsPlace: (on: boolean) => void;
  /** Gives the box the keyboard back. */
  refocus: () => void;
  openSendTo: () => void;
  closeSendTo: () => void;
  pickProject: (name: string) => void;
  activate: (key: string, alt: boolean) => void;
  toggleArchived: () => void;
  takeCalmer: (calmer: Calmer) => void;
  listenToIt: () => void;
  start: () => void;
}

/** Does what a key pressed anywhere in the panel does. */
export function onQuickKey(e: KeyboardEvent, panel: QuickKeys): void {
  const { askingDev, folded, picker, inSendTo, hearingHere, listen, projects, numbered, active, query, sendable, calmer, repoNow, macs, places, stepsPlace, setPicker, togglePicker, setStepsPlace, refocus, openSendTo, closeSendTo, pickProject, activate, toggleArchived, takeCalmer, listenToIt, start } = panel;
  // Switch to Codealong Dev? has the keys while it's up, Escape closing it (useEscape), and
  // so does any other dialog (Mark as done's about helpers).
  if (e.defaultPrevented || e.isComposing || askingDev || dialogUp()) return;
  // Folded, the panel has the keyboard only for the moment a click on the chip takes to open
  // it again. Escape puts it away, and so do ⌘Q and ⌘H, which mean the app in front (below).
  if (folded) {
    if (e.key === 'Escape' || (modsHeld(e, 'Cmd') && (e.key.toLowerCase() === 'q' || e.key.toLowerCase() === 'h'))) {
      e.preventDefault();
      void hideQuick();
    }
    return;
  }
  if (e.key === 'Escape') {
    e.preventDefault();
    if (picker) {
      setPicker(null);
      refocus();
    } else if (inSendTo) closeSendTo();
    else void hideQuick();
    return;
  }
  // The panel's other keys wait for a press that's being heard into the box.
  if (hearingHere) return;
  // ⌥⌘ digits switch projects, as in the main window. ⌥ changes the character, so they go by
  // the key's code.
  const digit = /^Digit(\d)$/.exec(e.code);
  if (digit && chordPressed(`Alt+Cmd+${digit[1]}`, e)) {
    e.preventDefault();
    const name = projectForDigit(Number(digit[1]), projects);
    if (name) pickProject(name);
    return;
  }
  if (!modsHeld(e, 'Cmd') && !modsHeld(e, 'Shift', 'Cmd')) return;
  if (/^[1-9]$/.test(e.key)) {
    const row = numbered[Number(e.key) - 1];
    if (row) {
      e.preventDefault();
      activate(row.key, false);
    }
    return;
  }
  const k = e.key.toLowerCase();
  // ⇧⌘A, while there's a search: the box on Sessions' heading. ⌘A alone selects what's typed.
  if (k === 'a' && e.shiftKey && query) {
    e.preventDefault();
    toggleArchived();
    return;
  }
  // ⌘F, under a prompt: Send to, and back.
  if (k === 'f' && !e.shiftKey && (inSendTo || sendable > 0)) {
    e.preventDefault();
    if (inSendTo) closeSendTo();
    else openSendTo();
    return;
  }
  // ⇧⌘D: the calm machine offered while this start's is strained.
  if (calmer && chordPressed(CALMER_CHORD, e)) {
    e.preventDefault();
    takeCalmer(calmer);
    return;
  }
  // The menus for a new session are put away in Send to, with their keys.
  // And set to listen, all but the repo's and the worktree's: its session starts on this
  // Mac, on the account Settings names for meetings.
  const startMenu = !inSendTo && (k === 't' || (k === 'b' && !!repoNow) || (!listen && (k === 'l' || (k === 'd' && macs.length > 0) || (k === 'o' && places.length > 1))));
  if (k === 'p' || startMenu) {
    e.preventDefault();
    // ⌘O opens the agent menu too, its ← → stepping where it runs, a row under its lines
    // (AgentMenu.tsx `AgentFoot`); with the menu up, ⌘O and ⌘L switch which row they step.
    const kind = k === 'p' ? 'project' : k === 'd' ? 'device' : k === 't' ? 'repo' : k === 'b' ? 'worktree' : 'agent';
    const forPlace = k === 'o';
    if (kind === 'agent' && picker === 'agent' && forPlace !== stepsPlace) setStepsPlace(forPlace);
    else {
      setStepsPlace(forPlace);
      togglePicker(kind);
    }
  } else if (k === 'q' || k === 'h') {
    // The menu bar is the other app's while the panel is up, so ⌘Q and ⌘H mean that app,
    // not Codealong: they close the panel instead of quitting or hiding Codealong.
    e.preventDefault();
    void hideQuick();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    if (inSendTo) activate(active, false);
    else if (listen) listenToIt();
    else start();
  }
}

/** Listens for the panel's keys, wherever the focus is in it. A menu's filter handles its own
 *  first. The listener is added again after every render, as it always was, with what that
 *  render had. */
export function useQuickKeys(panel: QuickKeys): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => onQuickKey(e, panel);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
}
