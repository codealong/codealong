// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { act, engine, mountHook, press, type, type Mounted } from './testing/dom';
import { createElement, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { setPlatform, type Platform } from './platform';
import { ALL_PROJECTS, type ReviewRow, type SessionRow, type TodoRow } from './quick';
import { onQuickBoxKey, useQuickKeys, type QuickBoxKeys, type QuickKeys } from './quickKeys';
import { COMPOSE, type Row, type SuggestRow } from './quickRows';
import type { SkillRow } from './skills';
import * as fx from './testing/fixtures';
import { todoItem } from './todos';

// The quick panel's keys as functions of the key and what the panel has (quickKeys.ts): the
// panel here is a record that writes down what it's asked to do, in words, and the key comes to
// it as it would on the page, through the window's listener (`useQuickKeys`) or a box's own
// handler (`onQuickBoxKey`). ui/Quick.test.ts presses the same keys on the page itself; here
// each rule is held on its own, with what the page can't easily be put into. Every test is
// pressed twice, as macOS has the keys and as Linux does.

const PLATFORMS: Platform[] = ['mac', 'linux'];

// Northwind's app has Kestrel, which needs you, and Wren, whose turn ended.
const KESTREL = 'claude:northwind:kestrel';
const WREN = 'claude:northwind:wren';
const kestrel = fx.session({ key: KESTREL, title: 'Kestrel', status: 'needs', live: true });
const wren = fx.session({ key: WREN, title: 'Wren', status: 'unread' });
const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind', repo: 'app', sessions: [kestrel, wren] });
const pr = fx.prItem({ repoSlug: 'northwind/app', number: 12, title: 'Kestrel login' });

const onKestrel: SessionRow = { kind: 'session', key: KESTREL, group: 'needs', lane: app, session: kestrel, marks: [] };
const onWren: SessionRow = { kind: 'session', key: WREN, group: 'unread', lane: app, session: wren, marks: [] };
const toReview: ReviewRow = { kind: 'review', key: pr.key, group: 'reviews', pr, marks: [] };
const aTodo: TodoRow = { kind: 'todo', key: 'todo:t1', group: 'ready', item: todoItem(fx.todo({ id: 't1' }), new Map(), new Map(), new Map()), waiting: false, marks: [] };
const sendToWren: SuggestRow = { kind: 'suggest', key: 'send:wren', group: 'suggested', s: { kind: 'send', key: 'send:wren', lane: app, session: wren, p: 0.7 } };
const startInApp: SuggestRow = { kind: 'suggest', key: 'start:app', group: 'suggested', s: { kind: 'start', key: 'start:app', lane: app, newWorktree: false, p: 0.3 } };
const deploy: SkillRow = { kind: 'skill', key: 'skill:@:deploy', group: 'skills', skill: { name: 'deploy', description: 'Ships what’s merged', typed: true, byModel: false, written: {}, uses: 0, lastUsed: 0 }, lane: null, accounts: ['northwind'], marks: [] };
/** Nine rows, for ⌘1 to ⌘9. */
const NINE: Row[] = Array.from({ length: 9 }, (_, i) => ({ ...onKestrel, key: `row${i + 1}` }));

/** What the panel was asked to do, in the order the keys asked; putting it away is the engine's
 *  (`hide_quick`), said here with the rest. */
const hidden = () => engine.sent('hide_quick').length;

let mounted: Mounted<void, void> | null = null;
let root: Root | null = null;
afterEach(() => {
  mounted = null;
  if (root) act(() => root?.unmount());
  root = null;
});

/** The panel with `over` in place of what it has as it opens: two rows, a repo to start in, two
 *  places to run in, no other machine. */
function panel(over: Partial<QuickKeys> = {}): string[] {
  const did: string[] = [];
  const keys: QuickKeys = {
    askingDev: false,
    folded: false,
    picker: null,
    inSendTo: false,
    hearingHere: null,
    listen: false,
    projects: [
      { name: 'Northwind', lanes: 1, repos: [] },
      { name: 'Atlas', lanes: 1, repos: [] },
    ],
    numbered: [onKestrel, onWren],
    active: KESTREL,
    query: '',
    sendable: 0,
    calmer: null,
    repoNow: {},
    macs: [],
    places: ['terminal', 'background'],
    stepsPlace: false,
    setPicker: (kind) => void did.push(kind ? `menu ${kind}` : 'menu away'),
    togglePicker: (kind) => void did.push(`toggle ${kind} menu`),
    setStepsPlace: (on) => void did.push(on ? 'arrows step the place' : 'arrows step the effort'),
    refocus: () => void did.push('keyboard to the box'),
    openSendTo: () => void did.push('send to'),
    closeSendTo: () => void did.push('back to the prompt'),
    pickProject: (name) => void did.push(`project ${name}`),
    activate: (key, alt) => void did.push(`do ${key}${alt ? ', the other way' : ''}`),
    toggleArchived: () => void did.push('archived or not'),
    takeCalmer: (calmer) => void did.push(`start on ${calmer.name}`),
    listenToIt: () => void did.push('listen'),
    start: () => void did.push('start'),
    ...over,
  };
  mounted?.unmount();
  mounted = mountHook(() => useQuickKeys(keys));
  return did;
}

/** What a chord does to a panel opened anew for it: what the panel was asked, `put away` when
 *  the engine was asked to hide it, and `left to the page` for a key it didn't take. */
function does(chord: string, over: Partial<QuickKeys> = {}): string[] {
  const before = hidden();
  const did = panel(over);
  const e = press(chord);
  return [...did, ...(hidden() > before ? ['put away'] : []), ...(e.defaultPrevented ? [] : ['left to the page'])];
}

const LEFT = ['left to the page'];
const mini = fx.deviceView({ name: 'Mac mini', online: true });
const calmer = { on: mini.short, name: 'Mac mini', load: { level: 'calm' as const, perCpu: 0.1, cpus: 10, memoryFree: 60, memoryShort: false }, lane: app };

describe.each(PLATFORMS)('the quick panel’s keys wherever the keyboard is in it, as %s presses them', (platform) => {
  it('is put away by Escape, and by ⌘Q and ⌘H with ⇧ held or not', () => {
    setPlatform(platform);
    for (const chord of ['Esc', 'Cmd+Q', 'Cmd+H', 'Shift+Cmd+Q', 'Shift+Cmd+H']) expect(does(chord), chord).toEqual(['put away']);
  });

  it('puts a menu away with Escape before anything else, the keyboard back in the box, and goes back from Send to before the panel goes', () => {
    setPlatform(platform);
    expect(does('Esc', { picker: 'repo' })).toEqual(['menu away', 'keyboard to the box']);
    expect(does('Esc', { picker: 'project', inSendTo: true })).toEqual(['menu away', 'keyboard to the box']);
    expect(does('Esc', { inSendTo: true })).toEqual(['back to the prompt']);
  });

  it('folded, is put away by Escape, ⌘Q and ⌘H alone, and hears nothing else', () => {
    setPlatform(platform);
    for (const chord of ['Esc', 'Cmd+Q', 'Cmd+H']) expect(does(chord, { folded: true }), chord).toEqual(['put away']);
    for (const chord of ['Shift+Cmd+Q', 'Cmd+P', 'Cmd+1', 'Alt+Cmd+1', 'Cmd+Enter', 'Cmd+F']) expect(does(chord, { folded: true, sendable: 2 }), chord).toEqual(LEFT);
    // Folded with a menu still up, Escape is the panel's, not the menu's.
    expect(does('Esc', { folded: true, picker: 'repo' })).toEqual(['put away']);
  });

  it('hears nothing while Switch to Codealong Dev? asks, a dialog is up, a key is being composed, or something took the key first', () => {
    setPlatform(platform);
    for (const chord of ['Esc', 'Cmd+P', 'Cmd+1', 'Cmd+Enter', 'Cmd+Q']) expect(does(chord, { askingDev: true }), chord).toEqual(LEFT);
    document.body.appendChild(document.createElement('div')).setAttribute('aria-modal', 'true');
    for (const chord of ['Esc', 'Cmd+P', 'Cmd+Enter']) expect(does(chord), chord).toEqual(LEFT);
    document.body.replaceChildren();
    const did = panel();
    expect([type({ key: 'Escape', code: 'Escape', isComposing: true }).defaultPrevented, did, hidden()]).toEqual([false, [], 0]);
    // A menu's filter, say, which takes its own Enter before the panel hears it.
    const taken = (e: KeyboardEvent) => e.preventDefault();
    window.addEventListener('keydown', taken, true);
    const then = panel();
    press('Cmd+Enter');
    press('Esc');
    window.removeEventListener('keydown', taken, true);
    expect([then, hidden()]).toEqual([[], 0]);
  });

  it('waits, all but Escape, while a press is heard into the box', () => {
    setPlatform(platform);
    const hearing = { hearingHere: { id: 7 } };
    for (const chord of ['Cmd+P', 'Cmd+1', 'Alt+Cmd+1', 'Cmd+Enter', 'Cmd+Q', 'Cmd+F']) expect(does(chord, { ...hearing, sendable: 2 }), chord).toEqual(LEFT);
    expect(does('Esc', hearing)).toEqual(['put away']);
    expect(does('Esc', { ...hearing, picker: 'agent' })).toEqual(['menu away', 'keyboard to the box']);
  });

  it('switches project with ⌥⌘ and a digit, 0 being all of them, and takes a digit with no project for itself', () => {
    setPlatform(platform);
    expect(does('Alt+Cmd+1')).toEqual(['project Northwind']);
    expect(does('Alt+Cmd+2')).toEqual(['project Atlas']);
    expect(does('Alt+Cmd+0')).toEqual([`project ${ALL_PROJECTS}`]);
    expect(does('Alt+Cmd+7')).toEqual([]);
    // In Send to and set to listen too.
    expect(does('Alt+Cmd+2', { inSendTo: true, listen: true })).toEqual(['project Atlas']);
  });

  it('does a row with ⌘ and its number, the first nine, and leaves a number with no row alone', () => {
    setPlatform(platform);
    expect(NINE.map((_, i) => does(`Cmd+${i + 1}`, { numbered: NINE }))).toEqual(NINE.map((r) => [`do ${r.key}`]));
    expect([does('Cmd+3'), does('Cmd+9'), does('Cmd+0', { numbered: NINE })]).toEqual([LEFT, LEFT, LEFT]);
    // In Send to they do its rows.
    expect(does('Cmd+2', { inSendTo: true })).toEqual([`do ${WREN}`]);
  });

  it('lists archived sessions or leaves them out with ⇧⌘A while something is looked for, and leaves ⌘A to the box', () => {
    setPlatform(platform);
    expect(does('Shift+Cmd+A', { query: 'kestrel' })).toEqual(['archived or not']);
    expect([does('Shift+Cmd+A'), does('Cmd+A', { query: 'kestrel' }), does('Cmd+A')]).toEqual([LEFT, LEFT, LEFT]);
  });

  it('goes to Send to with ⌘F while what’s typed could be sent to a session, and back with ⌘F from there', () => {
    setPlatform(platform);
    expect(does('Cmd+F', { sendable: 3 })).toEqual(['send to']);
    expect(does('Cmd+F', { inSendTo: true })).toEqual(['back to the prompt']);
    expect(does('Cmd+F', { inSendTo: true, sendable: 3 })).toEqual(['back to the prompt']);
    expect([does('Cmd+F'), does('Shift+Cmd+F', { sendable: 3 }), does('Shift+Cmd+F', { inSendTo: true })]).toEqual([LEFT, LEFT, LEFT]);
  });

  it('takes the calmer machine offered with ⇧⌘D, which is ⌘D’s menu when none is', () => {
    setPlatform(platform);
    expect(does('Shift+Cmd+D', { calmer, macs: [mini] })).toEqual(['start on Mac mini']);
    expect(does('Shift+Cmd+D', { macs: [mini] })).toEqual(['arrows step the effort', 'toggle device menu']);
    expect(does('Shift+Cmd+D')).toEqual(LEFT);
    // ⌘D alone is always the menu.
    expect(does('Cmd+D', { calmer, macs: [mini] })).toEqual(['arrows step the effort', 'toggle device menu']);
  });

  it('opens each menu with its key, the agent’s for where it runs with ⌘O', () => {
    setPlatform(platform);
    const all = { macs: [mini] };
    expect(Object.fromEntries(['P', 'D', 'T', 'B', 'L', 'O'].map((k) => [k, does(`Cmd+${k}`, all)]))).toEqual({
      P: ['arrows step the effort', 'toggle project menu'],
      D: ['arrows step the effort', 'toggle device menu'],
      T: ['arrows step the effort', 'toggle repo menu'],
      B: ['arrows step the effort', 'toggle worktree menu'],
      L: ['arrows step the effort', 'toggle agent menu'],
      O: ['arrows step the place', 'toggle agent menu'],
    });
  });

  it('with the agent menu up, ⌘O and ⌘L say which its arrows step, and the one they step already puts it away', () => {
    setPlatform(platform);
    expect(does('Cmd+O', { picker: 'agent' })).toEqual(['arrows step the place']);
    expect(does('Cmd+L', { picker: 'agent' })).toEqual(['arrows step the effort', 'toggle agent menu']);
    expect(does('Cmd+L', { picker: 'agent', stepsPlace: true })).toEqual(['arrows step the effort']);
    expect(does('Cmd+O', { picker: 'agent', stepsPlace: true })).toEqual(['arrows step the place', 'toggle agent menu']);
    // From another menu, ⌘O goes to the agent's.
    expect(does('Cmd+O', { picker: 'repo' })).toEqual(['arrows step the place', 'toggle agent menu']);
  });

  it('leaves a menu’s key alone where it has nothing to pick: no repo, no other machine, one place to run in', () => {
    setPlatform(platform);
    expect([does('Cmd+B', { repoNow: null }), does('Cmd+D'), does('Cmd+O', { places: ['terminal'] }), does('Cmd+O', { places: [] })]).toEqual([LEFT, LEFT, LEFT, LEFT]);
  });

  it('has only the project menu’s key in Send to, and only the project’s, repo’s and worktree’s set to listen', () => {
    setPlatform(platform);
    const all = { macs: [mini] };
    const keys = (over: Partial<QuickKeys>) => ['P', 'D', 'T', 'B', 'L', 'O'].filter((k) => !does(`Cmd+${k}`, { ...all, ...over }).includes('left to the page')).join('');
    expect([keys({}), keys({ inSendTo: true }), keys({ listen: true }), keys({ inSendTo: true, listen: true })]).toEqual(['PDTBLO', 'P', 'PTB', 'P']);
  });

  it('starts with ⌘⏎, with ⇧ held too: a session on what’s typed, the listening when set to listen, and in Send to the row selected', () => {
    setPlatform(platform);
    for (const chord of ['Cmd+Enter', 'Shift+Cmd+Enter']) {
      expect(does(chord), chord).toEqual(['start']);
      expect(does(chord, { listen: true }), chord).toEqual(['listen']);
      expect(does(chord, { inSendTo: true, active: WREN }), chord).toEqual([`do ${WREN}`]);
      expect(does(chord, { inSendTo: true, listen: true }), chord).toEqual([`do ${KESTREL}`]);
    }
  });

  it('leaves every other key to the page', () => {
    setPlatform(platform);
    for (const chord of ['Enter', 'Tab', 'Down', 'a', 'p', '1', 'Cmd+K', 'Cmd+W', 'Cmd+Z', 'Alt+P', 'Alt+Enter', 'Ctrl+Cmd+P']) expect(does(chord), chord).toEqual(LEFT);
  });
});

/** What the box holds and where the panel is, for its keys. */
interface Box {
  text?: string;
  /** Where the caret is in it; its end when left out. */
  caret?: number;
}

/** A box with `over` in place of what the panel has as it opens: Kestrel, a review and Wren
 *  listed over the new session's row, the first selected, nothing typed. */
function box(over: Partial<QuickBoxKeys> = {}, { text = '', caret = text.length }: Box = {}) {
  const did: string[] = [];
  const rows: Row[] = over.rows ?? [onKestrel, toReview, onWren];
  const keys: QuickBoxKeys = {
    hearingHere: null,
    listen: false,
    inSendTo: false,
    rows,
    keys: [...rows.map((r) => r.key), COMPOSE],
    active: rows[0]?.key ?? COMPOSE,
    recallable: [],
    recall: null,
    attached: [],
    setError: (message) => void did.push(`say “${message}”`),
    listenToIt: () => void did.push('listen'),
    activate: (key, alt) => void did.push(`do ${key}${alt ? ', the other way' : ''}`),
    closeSendTo: () => void did.push('back to the prompt'),
    move: (delta) => void did.push(`move ${delta}`),
    pickSkill: (row) => void did.push(`finish /${row.skill.name}`),
    answerSession: (_lane, session) => void did.push(`answer ${session.title}`),
    answerIn: (key) => void did.push(`in the waiting window: ${key}`),
    start: () => void did.push('start'),
    recallAt: (at) => void did.push(at < 0 ? 'the empty box again' : `bring back prompt ${at}`),
    ...over,
  };
  if (root) act(() => root?.unmount());
  document.body.replaceChildren();
  root = createRoot(document.body.appendChild(document.createElement('div')));
  act(() => root?.render(createElement('textarea', { defaultValue: text, onKeyDown: (e: ReactKeyboardEvent<HTMLTextAreaElement>) => onQuickBoxKey(e, keys) })));
  const el = document.querySelector('textarea')!;
  el.focus();
  el.setSelectionRange(caret, caret);
  return did;
}

/** What a chord pressed in a box opened anew for it does, as `does` says it for the panel. */
function inBox(chord: string, over: Partial<QuickBoxKeys> = {}, holds: Box = {}): string[] {
  const did = box(over, holds);
  const e = press(chord);
  return [...did, ...(e.defaultPrevented ? [] : ['left to the page'])];
}

describe.each(PLATFORMS)('the quick panel’s keys in its box, as %s presses them', (platform) => {
  it('does the row selected with ⏎, the other way with ⌥⏎, starts with ⌘⏎, and leaves ⇧⏎ to make a new line', () => {
    setPlatform(platform);
    expect(inBox('Enter')).toEqual([`do ${KESTREL}`]);
    expect(inBox('Enter', { active: COMPOSE })).toEqual([`do ${COMPOSE}`]);
    expect(inBox('Alt+Enter', { active: WREN })).toEqual([`do ${WREN}, the other way`]);
    expect(inBox('Cmd+Enter')).toEqual(['start']);
    expect(inBox('Alt+Cmd+Enter')).toEqual(['start']);
    expect([inBox('Shift+Enter'), inBox('Shift+Cmd+Enter')]).toEqual([LEFT, LEFT]);
  });

  it('answers a session with ⇥, one a suggestion names too, reviews a PR, finishes a skill’s name, and leaves ⇥ alone elsewhere', () => {
    setPlatform(platform);
    const rows = [sendToWren, startInApp, onKestrel, toReview, aTodo, deploy];
    const on = (active: string) => inBox('Tab', { rows, active });
    expect(on(KESTREL)).toEqual(['answer Kestrel']);
    expect(on(sendToWren.key)).toEqual(['answer Wren']);
    expect(on(toReview.key)).toEqual([`in the waiting window: ${pr.key}`]);
    expect(on(deploy.key)).toEqual(['finish /deploy']);
    expect([on(startInApp.key), on(aTodo.key), on(COMPOSE)]).toEqual([LEFT, LEFT, LEFT]);
    for (const chord of ['Shift+Tab', 'Alt+Tab', 'Cmd+Tab']) expect(inBox(chord), chord).toEqual(LEFT);
  });

  it('moves through the rows with ↓ and ↑, and not with ⇧, ⌥ or ⌘ held', () => {
    setPlatform(platform);
    expect([inBox('Down'), inBox('Up', { active: WREN })]).toEqual([['move 1'], ['move -1']]);
    for (const chord of ['Shift+Down', 'Alt+Down', 'Cmd+Down', 'Shift+Up', 'Alt+Up', 'Cmd+Up']) expect(inBox(chord, { active: WREN }), chord).toEqual(LEFT);
  });

  it('moves through the rows with ⌃N and ⌃P where text fields have them, on macOS, and not with ⌘ too', () => {
    setPlatform(platform);
    const mac = platform === 'mac';
    expect([inBox('Ctrl+N'), inBox('Ctrl+P')]).toEqual(mac ? [['move 1'], ['move -1']] : [LEFT, LEFT]);
    expect([inBox('Ctrl+Cmd+N'), inBox('Ctrl+Cmd+P')]).toEqual([LEFT, LEFT]);
  });

  it('brings back the draft or the last prompt with ↑ in the empty box on the first row, and not from further down or with something in the box', () => {
    setPlatform(platform);
    const earlier = { recallable: ['the second prompt', 'the first prompt'] };
    expect(inBox('Up', earlier)).toEqual(['bring back prompt 0']);
    // With nothing listed over the new session's row, that's the first.
    expect(inBox('Up', { ...earlier, rows: [] })).toEqual(['bring back prompt 0']);
    expect(inBox('Up', { ...earlier, active: WREN })).toEqual(['move -1']);
    expect(inBox('Up', earlier, { text: 'kestrel' })).toEqual(['move -1']);
    expect(inBox('Up', { ...earlier, attached: [{ kind: 'image', path: 'images/window.png', name: 'window.png' }] })).toEqual(['move -1']);
    // ↓ in the empty box is the list's.
    expect(inBox('Down', earlier)).toEqual(['move 1']);
    // With no draft and no earlier prompt the key is taken, and nothing comes back.
    expect(inBox('Up')).toEqual([]);
    for (const chord of ['Shift+Up', 'Alt+Up', 'Cmd+Up']) expect(inBox(chord, earlier), chord).toEqual(LEFT);
  });

  it('walks on through earlier prompts while the box holds the one brought back, ↓ going forward to the empty box, and stops at the first', () => {
    setPlatform(platform);
    const earlier = { recallable: ['the second prompt', 'the first prompt'] };
    expect(inBox('Up', { ...earlier, recall: 0 }, { text: 'the second prompt' })).toEqual(['bring back prompt 1']);
    expect(inBox('Down', { ...earlier, recall: 1 }, { text: 'the first prompt' })).toEqual(['bring back prompt 0']);
    expect(inBox('Down', { ...earlier, recall: 0 }, { text: 'the second prompt' })).toEqual(['the empty box again']);
    expect(inBox('Up', { ...earlier, recall: 1 }, { text: 'the first prompt' })).toEqual([]);
    // From a row further down too: it's the box that says the walk is on.
    expect(inBox('Up', { ...earlier, recall: 0, active: WREN }, { text: 'the second prompt' })).toEqual(['bring back prompt 1']);
    // Changed, what came back is a prompt like any other, and the arrows are the list's.
    expect(inBox('Up', { ...earlier, recall: 0, active: WREN }, { text: 'the second prompt, changed' })).toEqual(['move -1']);
    expect(inBox('Down', { ...earlier, recall: 0 }, { text: 'the second prompt, changed' })).toEqual(['move 1']);
  });

  it('leaves ↑ and ↓ to the caret inside a prompt of several lines, and moves through the rows from its first and last line', () => {
    setPlatform(platform);
    const text = 'one line\nand another';
    const at = (caret: number, chord: string) => inBox(chord, { active: WREN }, { text, caret });
    expect([at(text.length, 'Up'), at(text.length, 'Down')]).toEqual([LEFT, ['move 1']]);
    expect([at(0, 'Down'), at(0, 'Up')]).toEqual([LEFT, ['move -1']]);
  });

  it('in Send to, does the row selected with ⏎ and ⌘⏎, the other way with ⌥⏎, and takes ⇧⏎ without a new line', () => {
    setPlatform(platform);
    const sendTo = { inSendTo: true, active: WREN };
    expect(inBox('Enter', sendTo)).toEqual([`do ${WREN}`]);
    expect(inBox('Cmd+Enter', sendTo)).toEqual([`do ${WREN}`]);
    expect(inBox('Alt+Enter', sendTo)).toEqual([`do ${WREN}, the other way`]);
    expect(inBox('Shift+Enter', sendTo)).toEqual([]);
  });

  it('in Send to, goes back to the prompt with ⌫ in the empty box alone, moves with the arrows and never walks back through earlier prompts', () => {
    setPlatform(platform);
    const sendTo = { inSendTo: true, recallable: ['the second prompt'] };
    expect(inBox('Backspace', sendTo)).toEqual(['back to the prompt']);
    expect([inBox('Backspace', sendTo, { text: 'wre' }), inBox('Shift+Backspace', sendTo), inBox('Alt+Backspace', sendTo), inBox('Cmd+Backspace', sendTo)]).toEqual([LEFT, LEFT, LEFT, LEFT]);
    expect([inBox('Up', sendTo), inBox('Down', sendTo)]).toEqual([['move -1'], ['move 1']]);
    for (const chord of ['Shift+Down', 'Alt+Down', 'Cmd+Down']) expect(inBox(chord, sendTo), chord).toEqual(LEFT);
    const mac = platform === 'mac';
    expect([inBox('Ctrl+N', sendTo), inBox('Ctrl+P', sendTo)]).toEqual(mac ? [['move 1'], ['move -1']] : [LEFT, LEFT]);
    // ⇥ answers the session selected there too.
    expect(inBox('Tab', sendTo)).toEqual(['answer Kestrel']);
  });

  it('set to listen, starts listening with ⏎, with ⌘ or ⌥ held too, and has no other key', () => {
    setPlatform(platform);
    const listen = { listen: true, recallable: ['the second prompt'] };
    for (const chord of ['Enter', 'Cmd+Enter', 'Alt+Enter']) expect(inBox(chord, listen), chord).toEqual(['listen']);
    for (const chord of ['Shift+Enter', 'Up', 'Down', 'Tab', 'Ctrl+N', 'Backspace']) expect(inBox(chord, listen), chord).toEqual(LEFT);
    // Listening comes before Send to.
    expect(inBox('Enter', { ...listen, inSendTo: true })).toEqual(['listen']);
  });

  it('while a press is heard into it, ends the press with ⏎ and does nothing else, whatever the panel is set to', () => {
    setPlatform(platform);
    const hearing = { hearingHere: { id: 7 }, recallable: ['the second prompt'] };
    for (const over of [hearing, { ...hearing, listen: true }, { ...hearing, inSendTo: true }]) {
      const before = engine.sent('talk').length;
      expect(inBox('Enter', over)).toEqual([]);
      expect(engine.sent('talk').slice(before)).toEqual([{ on: false, drop: null }]);
      for (const chord of ['Up', 'Down', 'Tab', 'Backspace']) expect(inBox(chord, over), chord).toEqual(LEFT);
    }
  });

  it('hears nothing while a key is still being composed', () => {
    setPlatform(platform);
    for (const init of [{ key: 'Enter', code: 'Enter' }, { key: 'ArrowDown', code: 'ArrowDown' }, { key: 'Tab', code: 'Tab' }]) {
      const did = box();
      expect([type({ ...init, isComposing: true }).defaultPrevented, did], init.key).toEqual([false, []]);
    }
  });
});
