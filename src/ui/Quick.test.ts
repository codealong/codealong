// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { act, engine, press, settle, type } from '../testing/dom';
import { createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import type { QuickOpen } from '../api';
import { setPlatform, type Platform } from '../platform';
import { SEND_TO_PLACEHOLDER } from '../quick';
import * as fx from '../testing/fixtures';
import { withAttachments } from '../todos';
import { DEVICES_OFF, type Board, type DevicesView, type Skill, type StartOptions } from '../types';
import { Quick } from './Quick';

// The quick panel's keyboard, as a key pressed on its page comes to it: the whole page (Quick.tsx)
// mounted over the engine's stand-in and opened as quick.rs opens it (`window.__quickOpen`), with
// the keyboard where an open puts it, in the box. What a key did is read off the page (the row
// selected, the menu open, what the box holds) and off what the page asked of the engine. Every
// test is pressed twice, as macOS has the keys and as Linux does (platform.ts): the chords are
// written once, for macOS, and `press` sends what the platform's keyboard would.
//
// The keys are the box's own and the panel's, wherever the keyboard is in it. The tests go
// through the page, so they hold the keys wherever the code that has them is kept. Sessions,
// accounts and prompts here are made up.

const PLATFORMS: Platform[] = ['mac', 'linux'];

// Northwind's app has Kestrel, which needs you, and Wren, whose turn ended; Atlas's site has
// Cedar, at work. A todo is ready to start in the app, and a PR there waits on your review.
const APP = '/r/northwind/app';
const SITE = '/r/atlas/site';
const KESTREL = 'claude:northwind:kestrel';
const WREN = 'claude:northwind:wren';
const CEDAR = 'claude:atlas:cedar';
const REVIEW = 'northwind/app#12';
const TODO = 'todo:t1';
const COMPOSE = 'compose';
/** The rows of the panel with nothing typed, in order, the new session's last. */
const LISTED = [KESTREL, REVIEW, WREN, TODO, COMPOSE];
/** A prompt, which Send to can send, and a word, which only finds. */
const PROMPT = 'fix the login page';

function board(over: Partial<Board> = {}): Board {
  return fx.board({
    projects: [
      { name: 'Northwind', lanes: 1, repos: [{ name: 'app', lanes: 1, folder: false }] },
      { name: 'Atlas', lanes: 1, repos: [{ name: 'site', lanes: 1, folder: false }] },
    ],
    lanes: [
      fx.lane({
        id: APP,
        project: 'Northwind',
        repo: 'app',
        repoSlug: 'northwind/app',
        origin: 'main',
        sessions: [fx.session({ key: KESTREL, title: 'Kestrel', status: 'needs', live: true, pid: 4242 }), fx.session({ key: WREN, title: 'Wren', status: 'unread' })],
      }),
      fx.lane({ id: SITE, project: 'Atlas', repo: 'site', repoSlug: 'atlas/site', origin: 'main', sessions: [fx.session({ key: CEDAR, title: 'Cedar', status: 'working', live: true, pid: 4243 })] }),
    ],
    todos: [fx.todo({ id: 't1', title: 'Ship the login', project: 'Northwind', worktrees: [APP] })],
    prs: [fx.prItem({ repoSlug: 'northwind/app', number: 12, title: 'Kestrel login', project: 'Northwind', reviewRequested: true })],
    ...over,
  });
}

/** One account, which starts in the terminal or in the background. */
const OPTIONS: StartOptions = {
  accounts: [
    fx.startAccount({
      name: 'northwind',
      places: [
        { place: 'terminal', label: 'Terminal' },
        { place: 'background', label: 'Background' },
      ],
      chosen: 'terminal',
    }),
  ],
  codexProjects: [],
  claudeModels: [],
};

// A Mac mini is linked and connected, with a checkout of the app too.
const MINI = 'a1b2c3d4e5';
const MINI_APP = `@${MINI}:/r/northwind/app`;
const withMini = (over: Partial<Board> = {}): Board => board({ devices: { ...DEVICES_OFF, enabled: true }, ...over });
const mini = (over: Partial<DevicesView['devices'][number]> = {}): DevicesView =>
  fx.devicesView({
    devices: [fx.deviceView({ short: MINI, name: 'Mac mini', online: true, remoteControl: true, ...over })],
    lanes: [fx.lane({ id: MINI_APP, project: 'Northwind', repo: 'app', repoSlug: 'northwind/app', origin: 'main', path: '/r/northwind/app' })],
  });

/** What a panel is opened with: what quick.rs hands the page, and what the engine answers it. */
interface Opening {
  board?: Board;
  open?: QuickOpen;
  options?: StartOptions;
  devices?: DevicesView;
  /** Prompts sent before, newest first, as the panel keeps them. */
  history?: string[];
  /** The skills of a repo, by its folder. */
  skills?: { folder: string; skills: Skill[] };
}

let root: Root | null = null;
/** Takes the panel off the page, with what it showed ahead of the board (ahead.ts): a prompt it
 *  sent stays on its session in every window until a board shows it, and in the next panel too,
 *  so each is dropped as another window drops one. */
function unmount(): void {
  const ahead = engine.calls.flatMap((c) => (c.cmd === 'plugin:event|emit' && c.args.event === 'ahead' ? [(c.args.payload as { put?: { id: string } }).put?.id] : []));
  for (const id of new Set(ahead)) if (id) act(() => engine.emit('ahead', { from: 'elsewhere', drop: id }));
  if (root) act(() => root?.unmount());
  root = null;
}
afterEach(unmount);

/** What the page reads of the engine by itself, as it comes up and as it lists. */
const READS = ['get_accounts', 'quick_shortcut', 'desktop_pins', 'start_options', 'quick_ready', 'devices', 'device_start_options', 'observe', 'skills', 'search_text', 'search_devices', 'route_quick'];
/** What it keeps for the routing log of what you did with a prompt (routelog.rs), which isn't the keys' own. */
const NOTES = ['note_route'];
let told = 0;
/** What the page asked the engine to do since this was last read: each command with what it sent. */
function did(): string[] {
  const since = engine.calls.slice(told);
  told = engine.calls.length;
  return since.filter((c) => !READS.includes(c.cmd) && !NOTES.includes(c.cmd) && !c.cmd.startsWith('plugin:')).map((c) => `${c.cmd} ${JSON.stringify(c.args)}`);
}

const HIDE = 'hide_quick {}';
const showSession = (laneId: string, key: string) => `show_main ${JSON.stringify({ reveal: { kind: 'session', laneId, key } })}`;
const answer = (key: string) => `show_waiting ${JSON.stringify({ context: { key, rename: false } })}`;
const start = (prompt: string, laneId = APP) => `start_from_quick ${JSON.stringify({ spec: { laneId, account: 'northwind', place: 'terminal', prompt, model: null, effort: null, worktree: false, todos: [] } })}`;
const send = (key: string, text: string) => `send_to_session ${JSON.stringify({ key, text, now: false })}`;

/** The panel, opened as its shortcut opens it: on a page of its own, whatever a test had up before. */
async function panel(opening: Opening = {}): Promise<void> {
  unmount();
  document.body.replaceChildren();
  window.localStorage.clear();
  engine.window('quick');
  engine.answer('desktop_pins', []);
  engine.answer('start_options', opening.options ?? OPTIONS);
  engine.answer('device_start_options', opening.options ?? OPTIONS);
  engine.answer('send_to_session', { message: null, opened: false });
  // What sessions said is looked through a moment after typing pauses: nothing said matches.
  engine.answer('search_text', { hits: [], building: false, off: false });
  engine.answer('search_devices', []);
  engine.answer('skills', (args: Record<string, unknown>) => ({ folders: (args.folders as string[]).map((folder) => ({ folder, skills: folder === opening.skills?.folder ? opening.skills.skills : [] })), accounts: [] }));
  if (opening.devices) engine.answer('devices', opening.devices);
  if (opening.history) window.localStorage.setItem('codealong.quick.history', JSON.stringify(opening.history));
  // jsdom lays nothing out, so it has nothing to scroll.
  Element.prototype.scrollIntoView = () => {};
  root = createRoot(document.body.appendChild(document.createElement('div')));
  act(() => root?.render(createElement(Quick)));
  await settle();
  await open(opening.open, opening.board);
}

/** Opens the panel that's on the page, as quick.rs does each time its shortcut is pressed. */
async function open(context: QuickOpen = {}, on: Board = board()): Promise<void> {
  act(() => void window.__quickOpen?.(context, on));
  await settle();
  told = engine.calls.length;
}

const box = () => document.querySelector<HTMLTextAreaElement>('.ca-quick-input textarea')!;
/** The rows listed, the new session's among them, by their keys. */
const rows = () => Array.from(document.querySelectorAll<HTMLElement>('.ca-quick-list [data-row]'), (el) => el.dataset.row);
/** The row selected. */
const selected = () => document.querySelector<HTMLElement>('.ca-quick-list [data-row][aria-selected="true"]')?.dataset.row ?? null;
/** The menu that's open, by what its filter says it finds. */
const menu = () => document.querySelector<HTMLInputElement>('.ca-picker-filter')?.placeholder ?? null;
/** Whether the agent menu's ← → step where it runs. */
const stepsPlace = () => !!document.querySelector('.ca-picker-foot .ca-kbds');
const project = () => document.querySelector('.ca-quick-project span')?.textContent ?? null;
const inSendTo = () => box().placeholder === SEND_TO_PLACEHOLDER;
const folded = () => !!document.querySelector('.ca-quick-root.is-folded');

/** Types `text` into the box, in place of what it holds, as a keyboard would, the caret after it
 *  or at `caret`. */
async function write(text: string, caret = text.length): Promise<void> {
  const el = box();
  const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
  act(() => {
    set?.call(el, text);
    el.setSelectionRange(caret, caret);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await settle();
}

/** Lets `ms` go by, for what waits on typing to pause. */
const pause = (ms: number) => act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));

/** A key pressed: whether something took it, and what the page then asked of the engine. */
async function key(chord: string): Promise<[boolean, string[]]> {
  const taken = press(chord).defaultPrevented;
  await settle();
  return [taken, did()];
}

const MENUS = { project: 'Project', device: 'Machine', repo: 'Repo, a worktree by name, or a folder’s path', worktree: 'Worktree or branch', agent: 'Account or model' };

describe.each(PLATFORMS)('the quick panel’s keys wherever the keyboard is in it, as %s presses them', (platform) => {
  it('lists what waits on you, a todo ready to start and the new session’s row, the first selected and the keyboard in the box', async () => {
    setPlatform(platform);
    await panel();
    expect([rows(), selected(), document.activeElement === box(), project()]).toEqual([LISTED, KESTREL, true, 'All projects']);
  });

  it('is put away by Escape, and by ⌘Q and ⌘H, which mean the app in front, with ⇧ held or not', async () => {
    setPlatform(platform);
    for (const chord of ['Esc', 'Cmd+Q', 'Cmd+H', 'Shift+Cmd+Q', 'Shift+Cmd+H']) {
      await panel();
      expect(await key(chord), chord).toEqual([true, [HIDE]]);
    }
  });

  it('opens each menu with its key, and puts it away with the same key or with Escape, the panel staying', async () => {
    setPlatform(platform);
    await panel({ board: withMini(), devices: mini() });
    for (const [chord, says] of [['Cmd+P', MENUS.project], ['Cmd+D', MENUS.device], ['Cmd+T', MENUS.repo], ['Cmd+B', MENUS.worktree], ['Cmd+L', MENUS.agent], ['Cmd+O', MENUS.agent]]) {
      expect([await key(chord), menu()], chord).toEqual([[true, []], says]);
      expect([await key(chord), menu()], chord).toEqual([[true, []], null]);
      await key(chord);
      expect([await key('Esc'), menu()], chord).toEqual([[true, []], null]);
    }
  });

  it('puts a menu away with Escape from anywhere in the panel, not only from the menu’s own filter, and the panel with the next', async () => {
    setPlatform(platform);
    await panel();
    await key('Cmd+T');
    act(() => (document.activeElement as HTMLElement).blur());
    expect([document.activeElement === document.body, menu()]).toEqual([true, MENUS.repo]);
    expect([await key('Esc'), menu()]).toEqual([[true, []], null]);
    expect(await key('Esc')).toEqual([true, [HIDE]]);
  });

  it('goes from one menu to another with the other’s key', async () => {
    setPlatform(platform);
    await panel();
    await key('Cmd+P');
    expect([await key('Cmd+T'), menu()]).toEqual([[true, []], MENUS.repo]);
    expect([await key('Cmd+L'), menu()]).toEqual([[true, []], MENUS.agent]);
  });

  it('opens the agent menu for where it runs with ⌘O, and with it up ⌘O and ⌘L say which of the two its arrows step', async () => {
    setPlatform(platform);
    await panel();
    await key('Cmd+L');
    expect([menu(), stepsPlace()]).toEqual([MENUS.agent, false]);
    await key('Cmd+O');
    expect([menu(), stepsPlace()]).toEqual([MENUS.agent, true]);
    await key('Cmd+L');
    expect([menu(), stepsPlace()]).toEqual([MENUS.agent, false]);
    await key('Cmd+L');
    expect(menu()).toBeNull();
    await key('Cmd+O');
    expect([menu(), stepsPlace()]).toEqual([MENUS.agent, true]);
    await key('Cmd+O');
    expect(menu()).toBeNull();
  });

  it('leaves a menu’s key alone where there’s nothing for it to pick: no other machine, one place to run in, no repo', async () => {
    setPlatform(platform);
    const terminalOnly: StartOptions = { ...OPTIONS, accounts: [fx.startAccount({ name: 'northwind', places: [{ place: 'terminal', label: 'Terminal' }], chosen: 'terminal' })] };
    await panel({ options: terminalOnly });
    for (const chord of ['Cmd+D', 'Cmd+O']) expect([await key(chord), menu()], chord).toEqual([[false, []], null]);
    // General chat with no chats on the board yet has no repo, so no worktree to pick in one.
    await panel({ open: { chat: true } });
    expect([await key('Cmd+B'), menu()]).toEqual([[false, []], null]);
    expect([await key('Cmd+T'), menu()]).toEqual([[true, []], MENUS.repo]);
  });

  it('leaves a menu’s letter and a row’s number alone with ⌃ held as well as ⌘', async () => {
    setPlatform(platform);
    await panel({ board: withMini(), devices: mini() });
    for (const chord of ['Ctrl+Cmd+P', 'Ctrl+Cmd+D', 'Ctrl+Cmd+T', 'Ctrl+Cmd+L', 'Ctrl+Cmd+1']) expect([await key(chord), menu(), rows()], chord).toEqual([[false, []], null, LISTED]);
  });

  it('does a row with ⌘ and its number: a session shows in Codealong, a review in the waiting window, a todo gets a session ready', async () => {
    setPlatform(platform);
    await panel();
    expect(await key('Cmd+1')).toEqual([true, [HIDE, showSession(APP, KESTREL)]]);
    await panel();
    expect(await key('Cmd+2')).toEqual([true, [HIDE, answer(REVIEW)]]);
    await panel();
    expect(await key('Cmd+3')).toEqual([true, [HIDE, showSession(APP, WREN)]]);
    await panel();
    expect(await key('Cmd+4')).toEqual([true, []]);
    expect([rows(), selected(), box().value.split('\n')[0]]).toEqual([[COMPOSE], COMPOSE, 'Ship the login']);
    // The new session's row has ⌘⏎, not a number, and there's no fifth row.
    await panel();
    expect([await key('Cmd+5'), await key('Cmd+9'), rows()]).toEqual([[false, []], [false, []], LISTED]);
  });

  it('switches project with ⌥⌘ and a digit, ⌥⌘0 going back to all of them, and takes a digit that has no project for itself', async () => {
    setPlatform(platform);
    await panel();
    expect([await key('Alt+Cmd+2'), project(), rows()]).toEqual([[true, []], 'Atlas', [CEDAR, COMPOSE]]);
    expect([await key('Alt+Cmd+1'), project(), rows()]).toEqual([[true, []], 'Northwind', LISTED]);
    expect([await key('Alt+Cmd+9'), project()]).toEqual([[true, []], 'Northwind']);
    expect([await key('Alt+Cmd+0'), project()]).toEqual([[true, []], 'All projects']);
  });

  it('lists archived sessions in a search, or leaves them out, with ⇧⌘A, and leaves ⇧⌘A and ⌘A alone with nothing looked for', async () => {
    setPlatform(platform);
    const archived = () => document.querySelector<HTMLInputElement>('.ca-quick-archived input')?.checked ?? null;
    await panel();
    expect([await key('Shift+Cmd+A'), archived()]).toEqual([[false, []], null]);
    await write('kestrel');
    expect(archived()).toBe(false);
    expect([await key('Shift+Cmd+A'), archived()]).toEqual([[true, []], true]);
    expect([await key('Shift+Cmd+A'), archived()]).toEqual([[true, []], false]);
    expect([await key('Cmd+A'), archived()]).toEqual([[false, []], false]);
  });

  it('goes to Send to and back with ⌘F under a prompt, and leaves ⌘F alone under a word or two', async () => {
    setPlatform(platform);
    await panel();
    await write('kestrel');
    expect([await key('Cmd+F'), inSendTo()]).toEqual([[false, []], false]);
    await write(PROMPT);
    expect([rows(), selected()]).toEqual([['sendto', COMPOSE], 'sendto']);
    expect([await key('Cmd+F'), inSendTo(), rows(), selected(), box().value]).toEqual([[true, []], true, [CEDAR, KESTREL, WREN], CEDAR, '']);
    expect([await key('Cmd+F'), inSendTo(), box().value]).toEqual([[true, []], false, PROMPT]);
    // ⇧⌘F is not Send to's.
    expect([await key('Shift+Cmd+F'), inSendTo()]).toEqual([[false, []], false]);
  });

  it('in Send to, goes back to the prompt with Escape, keeps the project menu’s key and puts the new session’s menus away with theirs', async () => {
    setPlatform(platform);
    await panel({ board: withMini(), devices: mini() });
    await write(PROMPT);
    await key('Cmd+F');
    for (const chord of ['Cmd+D', 'Cmd+T', 'Cmd+B', 'Cmd+L', 'Cmd+O']) expect([await key(chord), menu(), inSendTo()], chord).toEqual([[false, []], null, true]);
    expect([await key('Cmd+P'), menu()]).toEqual([[true, []], MENUS.project]);
    await key('Cmd+P');
    expect([await key('Esc'), inSendTo(), box().value]).toEqual([[true, []], false, PROMPT]);
  });

  it('starts a session on what’s typed with ⌘⏎, from the box and from anywhere else in the panel, with ⇧ held too', async () => {
    setPlatform(platform);
    for (const chord of ['Cmd+Enter', 'Shift+Cmd+Enter']) {
      await panel();
      await write(PROMPT);
      expect(await key(chord), `${chord} in the box`).toEqual([true, [HIDE, start(PROMPT)]]);
      await panel();
      await write(PROMPT);
      act(() => box().blur());
      expect(await key(chord), `${chord} elsewhere`).toEqual([true, [HIDE, start(PROMPT)]]);
    }
  });

  it('sends to the session selected with ⌘⏎ in Send to, from anywhere in the panel', async () => {
    setPlatform(platform);
    await panel();
    await write(PROMPT);
    await key('Cmd+F');
    act(() => box().blur());
    expect(await key('Cmd+Enter')).toEqual([true, [HIDE, send(CEDAR, PROMPT)]]);
  });

  it('goes to a view of the main window with the view’s chord, put away unless it holds a prompt', async () => {
    setPlatform(platform);
    const view = (id: string) => `show_main ${JSON.stringify({ reveal: { kind: 'view', view: id } })}`;
    await panel();
    expect([await key('Shift+Cmd+P'), menu()]).toEqual([[true, [HIDE, view('prs')]], null]);
    await panel();
    await write(PROMPT);
    expect([await key('Shift+Cmd+T'), menu()]).toEqual([[true, [view('todo')]], null]);
  });

  it('takes ⇧⌘D and ⇧⌘O as ⌘D and ⌘O where no calmer machine is offered', async () => {
    setPlatform(platform);
    await panel({ board: withMini(), devices: mini() });
    expect([await key('Shift+Cmd+D'), menu()]).toEqual([[true, []], MENUS.device]);
    expect([await key('Shift+Cmd+O'), menu(), stepsPlace()]).toEqual([[true, []], MENUS.agent, true]);
  });

  it('takes the calm machine offered while this one is strained with ⇧⌘D, the new session’s row selected', async () => {
    setPlatform(platform);
    const calm = { level: 'calm', perCpu: 0.1, cpus: 10, memoryFree: 60, memoryShort: false } as const;
    await panel({ board: withMini({ load: { level: 'strained', perCpu: 9, cpus: 10, memoryFree: 5, memoryShort: true } }), devices: mini({ load: calm }) });
    const machine = () => document.querySelector('.ca-quick-bar .ca-quick-chip span')?.textContent ?? null;
    expect([machine(), !!document.querySelector('.ca-quick-calmer'), selected()]).toEqual(['This machine', true, KESTREL]);
    expect([await key('Shift+Cmd+D'), machine(), menu(), selected()]).toEqual([[true, []], 'Mac mini', null, COMPOSE]);
    expect(await key('Cmd+Enter')).toEqual([true, [HIDE, start('', MINI_APP)]]);
  });

  it('folded to its chip, is put away by Escape, ⌘Q and ⌘H and hears no other key of the panel’s', async () => {
    setPlatform(platform);
    for (const chord of ['Esc', 'Cmd+Q', 'Cmd+H']) {
      await panel();
      await write(PROMPT);
      act(() => void window.__quickFold?.());
      expect(folded()).toBe(true);
      for (const other of ['Cmd+P', 'Cmd+T', 'Cmd+1', 'Alt+Cmd+1']) expect([await key(other), menu(), project()], other).toEqual([[false, []], null, 'All projects']);
      expect(await key(chord), chord).toEqual([true, [HIDE]]);
    }
  });

  it('leaves every key to Switch to Codealong Dev? while it asks, Escape closing it and not the panel', async () => {
    setPlatform(platform);
    const asks = () => !!document.querySelector('.ca-devsetup');
    await panel({ open: { improve: { shots: [] } }, board: board({ improve: { project: 'Northwind', laneId: APP, installed: true } }) });
    expect(asks()).toBe(true);
    for (const chord of ['Cmd+P', 'Cmd+T', 'Cmd+1', 'Cmd+Q', 'Alt+Cmd+2']) expect([await key(chord), menu(), project()], chord).toEqual([[false, []], null, 'Northwind']);
    expect([(await key('Esc'))[1], asks()]).toEqual([[], false]);
    expect([await key('Cmd+P'), menu()]).toEqual([[true, []], MENUS.project]);
  });

  it('hears nothing while a key is still being composed', async () => {
    setPlatform(platform);
    await panel();
    for (const init of [{ key: 'Escape', code: 'Escape' }, { key: 'Enter', code: 'Enter' }, { key: 'ArrowDown', code: 'ArrowDown' }, { key: 'Tab', code: 'Tab' }]) {
      expect([type({ ...init, isComposing: true }).defaultPrevented, did(), selected()], init.key).toEqual([false, [], KESTREL]);
    }
  });

  it('waits, all but Escape, while a press is heard into the box, where ⏎ ends the press', async () => {
    setPlatform(platform);
    await panel({ board: board({ experimental: { boards: false, voice: true } }) });
    act(() => engine.emitTo('quick', 'talk', { id: 7, phase: 'listening', to: 'looking', text: 'fix the login', level: 0 }));
    expect([box().value, box().readOnly]).toEqual(['fix the login', true]);
    for (const chord of ['Cmd+P', 'Cmd+1', 'Alt+Cmd+1', 'Cmd+Enter', 'Down', 'Tab']) expect([await key(chord), menu(), selected(), project()], chord).toEqual([[chord === 'Cmd+Enter', chord === 'Cmd+Enter' ? ['talk {"on":false,"drop":null}'] : []], null, KESTREL, 'All projects']);
    expect(await key('Enter')).toEqual([true, ['talk {"on":false,"drop":null}']]);
    expect(await key('Esc')).toEqual([true, [HIDE]]);
  });
});

describe.each(PLATFORMS)('the quick panel’s keys in its box, as %s presses them', (platform) => {
  it('does the row selected with ⏎: a session shows in Codealong, with ⌥ it opens where it lives', async () => {
    setPlatform(platform);
    await panel();
    expect(await key('Enter')).toEqual([true, [HIDE, showSession(APP, KESTREL)]]);
    await panel();
    expect(await key('Alt+Enter')).toEqual([true, [HIDE, `open_from_bar ${JSON.stringify({ laneId: APP, targetId: `s|${KESTREL}|focus` })}`]]);
  });

  it('reviews a PR that waits on you with ⏎, and shows it in Codealong with ⌥⏎', async () => {
    setPlatform(platform);
    await panel();
    await key('Down');
    expect(selected()).toBe(REVIEW);
    expect(await key('Enter')).toEqual([true, [HIDE, answer(REVIEW)]]);
    await panel();
    await key('Down');
    expect(await key('Alt+Enter')).toEqual([true, [HIDE, `show_main ${JSON.stringify({ reveal: { kind: 'pr', key: REVIEW } })}`]]);
  });

  it('gets a session ready for a todo with ⏎, and shows the todo in Codealong with ⌥⏎', async () => {
    setPlatform(platform);
    await panel();
    for (let i = 0; i < 3; i++) await key('Down');
    expect(selected()).toBe(TODO);
    expect([await key('Enter'), rows(), selected(), box().value.split('\n')[0]]).toEqual([[true, []], [COMPOSE], COMPOSE, 'Ship the login']);
    // And ⏎ again starts it, for the todo, on the todo's prompt.
    const prompt = box().value;
    expect((await key('Enter'))[1]).toEqual([HIDE, `start_from_quick ${JSON.stringify({ spec: { laneId: APP, account: 'northwind', place: 'terminal', prompt, model: null, effort: null, worktree: false, todos: ['t1'] } })}`]);
    await panel();
    for (let i = 0; i < 3; i++) await key('Down');
    expect(await key('Alt+Enter')).toEqual([true, [HIDE, `show_main ${JSON.stringify({ reveal: { kind: 'todo', id: 't1' } })}`]]);
  });

  it('starts a session with ⏎ on the new session’s row, and makes a new line with ⇧⏎', async () => {
    setPlatform(platform);
    await panel();
    for (let i = 0; i < 4; i++) await key('Down');
    expect(selected()).toBe(COMPOSE);
    expect(await key('Shift+Enter')).toEqual([false, []]);
    expect(await key('Enter')).toEqual([true, [HIDE, start('')]]);
  });

  it('answers the session selected in the waiting window with ⇥, reviews a PR there, and leaves ⇥ alone on any other row', async () => {
    setPlatform(platform);
    await panel();
    expect(await key('Tab')).toEqual([true, [HIDE, answer(KESTREL)]]);
    await panel();
    await key('Down');
    expect(await key('Tab')).toEqual([true, [HIDE, answer(REVIEW)]]);
    await panel();
    for (let i = 0; i < 3; i++) await key('Down');
    expect([await key('Tab'), await key('Down'), await key('Tab'), selected()]).toEqual([[false, []], [true, []], [false, []], COMPOSE]);
    // ⇧⇥ moves the focus back, as anywhere.
    await panel();
    expect(await key('Shift+Tab')).toEqual([false, []]);
  });

  it('finishes a skill’s name with ⇥ or ⏎ on its row, the new session’s row then selected', async () => {
    setPlatform(platform);
    const deploy: Skill = { name: 'deploy', description: 'Ships what’s merged', typed: true, byModel: false, written: { claude: '/deploy' }, uses: 0, lastUsed: 0 };
    // In a repo of its own: a list of skills is kept a while by the folders it's of (useSkills.ts).
    const tools = fx.lane({ id: '/r/northwind/tools', project: 'Northwind', repo: 'tools', repoSlug: 'northwind/tools', origin: 'main' });
    for (const chord of ['Tab', 'Enter']) {
      await panel({ board: board({ lanes: [tools] }), skills: { folder: tools.path, skills: [deploy] } });
      await write('/dep');
      expect([rows().length, selected() === rows()[0], selected() === COMPOSE], chord).toEqual([2, true, false]);
      expect([await key(chord), box().value, selected()], chord).toEqual([[true, []], '/deploy ', COMPOSE]);
    }
  });

  it('moves through the rows with ↓ and ↑, stopping at the last, and leaves the arrows alone with ⇧, ⌥ or ⌘ held', async () => {
    setPlatform(platform);
    await panel();
    const walked = [];
    for (let i = 0; i < 5; i++) walked.push([(await key('Down'))[0], selected()]);
    expect(walked).toEqual([[true, REVIEW], [true, WREN], [true, TODO], [true, COMPOSE], [true, COMPOSE]]);
    expect([await key('Up'), selected()]).toEqual([[true, []], TODO]);
    for (const chord of ['Shift+Down', 'Alt+Down', 'Cmd+Down', 'Shift+Up', 'Alt+Up', 'Cmd+Up']) expect([await key(chord), selected()], chord).toEqual([[false, []], TODO]);
  });

  it('moves through the rows with ⌃N and ⌃P where text fields have them: on macOS', async () => {
    setPlatform(platform);
    await panel();
    const mac = platform === 'mac';
    expect([await key('Ctrl+N'), selected()]).toEqual([[mac, []], mac ? REVIEW : KESTREL]);
    await key('Down');
    expect([await key('Ctrl+P'), selected()]).toEqual([[mac, []], REVIEW]);
    // With ⌘ too it's no text field's key.
    expect([await key('Ctrl+Cmd+N'), selected()]).toEqual([[false, []], REVIEW]);
  });

  it('brings back earlier prompts with ↑ in the empty box, ↓ going forward again to the empty box', async () => {
    setPlatform(platform);
    await panel({ history: ['the second prompt', 'the first prompt'] });
    expect([await key('Up'), box().value]).toEqual([[true, []], 'the second prompt']);
    expect([await key('Up'), box().value]).toEqual([[true, []], 'the first prompt']);
    // Nothing earlier than the first.
    expect([await key('Up'), box().value]).toEqual([[true, []], 'the first prompt']);
    expect([await key('Down'), box().value]).toEqual([[true, []], 'the second prompt']);
    expect([await key('Down'), box().value, selected()]).toEqual([[true, []], '', KESTREL]);
    // From the empty box again ↓ is the list's.
    expect([await key('Down'), box().value, selected()]).toEqual([[true, []], '', REVIEW]);
    // And so is ↑, from a row further down.
    expect([await key('Up'), box().value, selected()]).toEqual([[true, []], '', KESTREL]);
  });

  it('brings nothing back with ⇧, ⌥ or ⌘ held, nor into a box that has something attached', async () => {
    setPlatform(platform);
    const history = ['the second prompt', 'the first prompt'];
    await panel({ history });
    for (const chord of ['Shift+Up', 'Alt+Up', 'Cmd+Up']) expect([await key(chord), box().value], chord).toEqual([[false, []], '']);
    // A picture attached and nothing written is a prompt all the same: ↑ is the list's.
    await panel({ history, open: { prompt: withAttachments('', [{ kind: 'image', path: 'images/0123456789abcdef.png', name: 'window' }]) } });
    expect([box().value, document.querySelectorAll('.ca-quick-attached img').length]).toEqual(['', 1]);
    expect([await key('Up'), box().value]).toEqual([[true, []], '']);
  });

  it('leaves the arrows to the list once what ↑ brought back is changed', async () => {
    setPlatform(platform);
    await panel({ history: ['the second prompt', 'the first prompt'] });
    await key('Up');
    await write('the second prompt, changed');
    expect([await key('Up'), box().value]).toEqual([[true, []], 'the second prompt, changed']);
  });

  it('brings back the draft you left with ↑, then the prompts before it', async () => {
    setPlatform(platform);
    await panel({ history: ['the first prompt'] });
    await write('a draft left unsent');
    await key('Esc');
    await open();
    expect(box().value).toBe('');
    expect([await key('Up'), box().value]).toEqual([[true, []], 'a draft left unsent']);
    expect([await key('Up'), box().value]).toEqual([[true, []], 'the first prompt']);
  });

  it('brings back a draft written over Improve Codealong’s pictures with them, in Codealong’s source, and puts the panel’s own place back as ↓ leaves it', async () => {
    setPlatform(platform);
    const source = board({ improve: { project: 'Atlas', laneId: SITE } });
    const pictures = () => document.querySelectorAll('.ca-quick-attached img').length;
    const repo = () => document.querySelector('.ca-quick-bar .ca-quick-chip span')?.textContent ?? null;
    await panel({ open: { improve: { shots: [{ kind: 'image', path: 'images/window.png', name: 'window.png' }] } }, board: source });
    expect([project(), repo(), pictures()]).toEqual(['Atlas', 'site', 1]);
    await write('make the bar a little wider');
    await key('Esc');
    await open({}, source);
    expect([project(), repo(), pictures(), box().value]).toEqual(['All projects', 'app', 0, '']);
    expect([await key('Up'), box().value, project(), repo(), pictures()]).toEqual([[true, []], 'make the bar a little wider', 'Atlas', 'site', 1]);
    expect([await key('Down'), box().value, project(), repo(), pictures()]).toEqual([[true, []], '', 'All projects', 'app', 0]);
  });

  it('leaves ↑ and ↓ to the caret within a prompt of several lines, and moves through the rows from its first and last', async () => {
    setPlatform(platform);
    await panel();
    await write('one line\nand another');
    const first = selected();
    expect([await key('Up'), selected()]).toEqual([[false, []], first]);
    expect([await key('Down'), selected() === first]).toEqual([[true, []], false]);
    await write('one line\nand another', 0);
    expect((await key('Down'))[0]).toBe(false);
    expect((await key('Up'))[0]).toBe(true);
  });

  it('in Send to, moves through the sessions with the arrows, sends with ⏎ and shows the session with ⌥⏎', async () => {
    setPlatform(platform);
    const sendTo = async () => {
      await panel();
      await write(PROMPT);
      await key('Cmd+F');
    };
    await sendTo();
    expect([await key('Down'), selected()]).toEqual([[true, []], KESTREL]);
    expect([await key('Up'), selected()]).toEqual([[true, []], CEDAR]);
    // Never back through earlier prompts, from the first row and an empty box.
    expect([await key('Up'), selected(), box().value]).toEqual([[true, []], CEDAR, '']);
    for (const chord of ['Shift+Down', 'Alt+Down', 'Cmd+Down']) expect([await key(chord), selected()], chord).toEqual([[false, []], CEDAR]);
    const mac = platform === 'mac';
    expect([await key('Ctrl+N'), selected()]).toEqual([[mac, []], mac ? KESTREL : CEDAR]);
    expect([await key('Ctrl+P'), selected()]).toEqual([[mac, []], CEDAR]);
    expect(await key('Enter')).toEqual([true, [HIDE, send(CEDAR, PROMPT)]]);
    await sendTo();
    expect(await key('Alt+Enter')).toEqual([true, [HIDE, showSession(SITE, CEDAR)]]);
    await sendTo();
    expect(await key('Cmd+Enter')).toEqual([true, [HIDE, send(CEDAR, PROMPT)]]);
    // ⇧⏎ makes no new line there, and sends nothing.
    await sendTo();
    expect([await key('Shift+Enter'), inSendTo()]).toEqual([[true, []], true]);
    // A session that waits on you is shown, to answer, rather than sent to.
    await sendTo();
    await key('Down');
    expect(await key('Enter')).toEqual([true, [HIDE, showSession(APP, KESTREL)]]);
  });

  it('in Send to, goes back to the prompt with ⌫ in the empty box, and answers the session selected with ⇥', async () => {
    setPlatform(platform);
    await panel();
    await write(PROMPT);
    await key('Cmd+F');
    await write('ked');
    expect([await key('Backspace'), inSendTo()]).toEqual([[false, []], true]);
    await write('');
    expect([await key('Shift+Backspace'), inSendTo()]).toEqual([[false, []], true]);
    expect([await key('Backspace'), inSendTo(), box().value]).toEqual([[true, []], false, PROMPT]);
    await key('Cmd+F');
    expect(await key('Tab')).toEqual([true, [HIDE, answer(CEDAR)]]);
  });

  it('does what Jev suggests for a prompt with ⏎ and with its number, and answers the session it names with ⇥', async () => {
    setPlatform(platform);
    const suggested = async () => {
      await panel({ board: board({ decisions: { default: 'jev', quick_panel: true, sessions: false, todos: false, pull_requests: false, prompts: false } }) });
      engine.answer('route_quick', { action: 'send', actions: [['send', 0.9]], sessions: [{ id: WREN, p: 0.9 }], noSession: 0.1, lanes: [], repos: [], ownWorktree: 0, ownWorktreeAsked: 0, habit: { new: 0, of: 0 }, duplicate: null, model: 'jev', ms: 5, inputTokens: 1 });
      await write(PROMPT);
      // Once typing has paused, what's typed goes to Jev.
      await pause(500);
      await settle();
      told = engine.calls.length;
    };
    await suggested();
    const first = rows()[0];
    expect([rows().length, selected() === first, first?.includes(WREN)]).toEqual([3, true, true]);
    expect(await key('Enter')).toEqual([true, [HIDE, send(WREN, PROMPT)]]);
    await suggested();
    expect(await key('Cmd+1')).toEqual([true, [HIDE, send(WREN, PROMPT)]]);
    await suggested();
    expect(await key('Alt+Enter')).toEqual([true, [HIDE, showSession(APP, WREN)]]);
    await suggested();
    expect(await key('Tab')).toEqual([true, [HIDE, answer(WREN)]]);
  });

  it('set to listen, starts listening with ⏎ or ⌘⏎, makes a new line with ⇧⏎ and has no list to move through', async () => {
    setPlatform(platform);
    // With no chats on the board, its session starts where a new one would: in the app.
    const listen = (prompt: string) => `listen_now ${JSON.stringify({ meeting: null, lane: APP, ask: { mic: true, system: true, apps: [], role: 'notes', prompt, start: false } })}`;
    for (const chord of ['Enter', 'Cmd+Enter']) {
      await panel({ open: { listen: true } });
      await write('the weekly sync');
      expect(rows()).toEqual([]);
      for (const other of ['Down', 'Up', 'Tab', 'Shift+Enter']) expect(await key(other), other).toEqual([false, []]);
      expect(await key(chord), chord).toEqual([true, [HIDE, listen('the weekly sync')]]);
    }
    // From elsewhere in the panel ⌘⏎ does too, and ⌘L, ⌘D and ⌘O have no menu to open.
    await panel({ open: { listen: true }, board: withMini(), devices: mini() });
    for (const chord of ['Cmd+L', 'Cmd+D', 'Cmd+O']) expect([await key(chord), menu()], chord).toEqual([[false, []], null]);
    expect([await key('Cmd+T'), menu()]).toEqual([[true, []], MENUS.repo]);
    await key('Cmd+T');
    act(() => box().blur());
    expect(await key('Cmd+Enter')).toEqual([true, [HIDE, listen('')]]);
  });
});
