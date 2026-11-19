// The engine, as a unit test has it: what stands in for Tauri under src/api.ts, the only place
// the windows talk to it. The code under test runs its real api.ts, and @tauri-apps/api's own
// `invoke` and `listen`, over this: a command is recorded and answered as the test says, and an
// event the test emits reaches who in the page would hear it from the engine: one sent to a
// window (`emitTo`) only a listener that window made for itself, or a plain one, which hears
// every window's. scripts/preview/fake-tauri.js does the same for the browser checks, with a
// board snapshotted from real data behind it; this one has nothing behind it but what a test
// puts there.
//
// api.ts reads whether Tauri is there once, as it loads (`inTauri`), so a test file imports this
// (or ./dom, which does) before anything that imports api.ts: first, after vitest. A file that
// has it the other way round fails in every test, saying so. It needs a page to put Tauri's
// internals on: the file starts with `// @vitest-environment jsdom`.
//
// Only tests import this file. Nothing the app's pages load reaches it, so it isn't in the bundle.

import { afterEach, beforeAll } from 'vitest';

/** A command the page sent, with its arguments. */
export interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

type Answer = (args: Record<string, unknown>) => unknown;

/** Who a listener hears, as Tauri keeps it: anyone (a plain `listen`), or what's sent to the
 *  window that made it (`getCurrentWebviewWindow().listen`). */
type Target = { kind: 'Any' } | { kind: string; label: string };

if (typeof window === 'undefined') throw new Error('src/testing/tauri.ts needs a page: start the test file with `// @vitest-environment jsdom`.');

const calls: Call[] = [];
const unanswered: string[] = [];
const answers = new Map<string, Answer>();
/** What a command answers when a test says nothing else, where null isn't the engine's answer:
 *  what the page reads by itself as a module loads or a session shows, before a test could
 *  answer. Each is what scripts/preview/fake-tauri.js answers with nothing behind it, and every
 *  read the tests here set off is on the list; a command that only does something answers null
 *  there too. `engine.unanswered` names what got null, for a new read to be found by. */
const STANDING: Record<string, Answer> = {
  // menus.ts reads the accounts as it loads, and the start options for a session that isn't running.
  get_accounts: () => [],
  start_options: () => ({ accounts: [], codexProjects: [], claudeModels: [] }),
  // side.ts reads what's kept of a session's side conversations as it hears of one, and which
  // have windows of their own as a window first asks about one.
  side_past: () => [],
  side_windows: () => [],
  // terminals.ts asks whether the window has the keyboard as it loads: as the page has it.
  window_keys: () => document.hasFocus(),
};
// Tauri's callbacks, by the id `transformCallback` gave each, and who listens to which event.
const callbacks = new Map<number, (data: unknown) => void>();
const listeners = new Map<number, { event: string; handler: number; target: Target }>();
let ids = 0;
/** The main window's label, which the page is unless a test says otherwise (`engine.window`). */
const MAIN = 'main';
let label = MAIN;
/** While listening is held (`engine.holdListening`): resolves as it's let go. */
let held: { gone: Promise<void>; release: () => void } | null = null;

const EVENTS = 'plugin:event|';

/** What Tauri's `invoke` comes to here. (Not named for it: commands.test.ts reads every call of
 *  one in src for the command it names.) */
function asked(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const listens = cmd === `${EVENTS}listen`;
  const unlistens = cmd === `${EVENTS}unlisten`;
  if (!listens && !unlistens) calls.push({ cmd, args });
  // Answered later than it's asked, as the engine answers: what the page does next doesn't have
  // the answer yet, and an event emitted before a listener is in doesn't reach it.
  if (listens) {
    return (held?.gone ?? Promise.resolve()).then(() => {
      const id = ++ids;
      listeners.set(id, { event: String(args.event), handler: Number(args.handler), target: (args.target as Target | undefined) ?? { kind: 'Any' } });
      return id;
    });
  }
  return Promise.resolve().then(() => {
    if (unlistens) {
      listeners.delete(Number(args.eventId));
      return null;
    }
    const answer = answers.get(cmd) ?? (Object.hasOwn(STANDING, cmd) ? STANDING[cmd] : undefined);
    if (answer) return answer(args);
    if (!cmd.startsWith(EVENTS) && !unanswered.includes(cmd)) unanswered.push(cmd);
    return null;
  });
}

/** Hands `payload` to each listener of `event` that hears what's sent to the window `to`, or
 *  to every one with no window named. */
function deliver(event: string, payload: unknown, to: string | null): void {
  for (const [id, listener] of [...listeners]) {
    const hears = to === null || listener.target.kind === 'Any' || ('label' in listener.target && listener.target.label === to);
    if (listener.event === event && hears) callbacks.get(listener.handler)?.({ event, id, payload });
  }
}

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke: asked,
    transformCallback: (callback?: (data: unknown) => void, once = false) => {
      const id = ++ids;
      callbacks.set(id, (data) => {
        if (once) callbacks.delete(id);
        callback?.(data);
      });
      return id;
    },
    unregisterCallback: (id: number) => void callbacks.delete(id),
    convertFileSrc: (path: string, protocol = 'asset') => `${protocol}://localhost/${encodeURIComponent(path)}`,
    // Read as it's asked, so a test can say which window its page is before the page asks.
    metadata: {
      get currentWindow() {
        return { label };
      },
      get currentWebview() {
        return { windowLabel: label, label };
      },
    },
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: (_event: string, id: number) => {
      const listener = listeners.get(id);
      if (listener) callbacks.delete(listener.handler);
    },
  },
});

export const engine = {
  /** Every command the page sent since the test began, in order. What it emits to other windows
   *  is among them (`plugin:event|emit`, `plugin:event|emit_to`); its listening isn't. */
  calls,
  /** The arguments of each `cmd` sent, in order. */
  sent: (cmd: string): Record<string, unknown>[] => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
  /** The commands sent since the test began that had no answer but null, each once: right for
   *  one that only does something, and wrong for a read the page shows the answer of, which
   *  wants an `answer` from the test (or a place in `STANDING`, for one many tests set off). */
  unanswered,
  /** What `cmd` answers from now until the test ends: a value, or what a function makes of the
   *  arguments (a promise the test settles, a throw for one the engine turns down). A command
   *  with no answer resolves null, but for the few in `STANDING`. */
  answer: (cmd: string, answer: unknown): void => void answers.set(cmd, typeof answer === 'function' ? (answer as Answer) : () => answer),
  /** Which window the page is, by its label (bar.rs, waiting.rs, peek.rs and quick.rs have
   *  theirs): the main window's until a test says. Said before the page listens, since a
   *  listener a window makes for itself hears what's sent to the label it had then. */
  window: (is: string): void => {
    label = is;
  },
  /** Emits an event as the engine does to every window (`app.emit`): everyone listening to it
   *  hears it, in the order they began to. */
  emit: (event: string, payload: unknown): void => deliver(event, payload, null),
  /** Emits an event to the window `to` alone, as the engine does with `emit_to`: only a
   *  listener that window made for itself hears it, or a plain one, which hears every window's. */
  emitTo: (to: string, event: string, payload: unknown): void => deliver(event, payload, to),
  /** How many in the page listen to an event now. */
  listening: (event: string): number => [...listeners.values()].filter((l) => l.event === event && callbacks.has(l.handler)).length,
  /** Keeps what the page asks to listen to from being in, as the engine takes a moment to put
   *  it in, until the function this returns is called: nothing emitted meanwhile reaches it,
   *  and what waits on the listening waits. */
  holdListening: (): (() => void) => {
    if (!held) {
      let release = () => {};
      const gone = new Promise<void>((resolve) => (release = resolve));
      held = { gone, release };
    }
    const mine = held;
    return () => {
      if (held === mine) held = null;
      mine.release();
    };
  },
};

// Each test starts with nothing sent and nothing answered, as the main window, with listening
// let through. Who listens is the page's own to end.
afterEach(() => {
  calls.length = 0;
  unanswered.length = 0;
  answers.clear();
  label = MAIN;
  held?.release();
  held = null;
});

beforeAll(async () => {
  const { inTauri } = await import('../api');
  if (!inTauri) throw new Error('src/api.ts loaded before src/testing/tauri.ts, so it took the page for a browser preview: import ./testing/tauri (or ./testing/dom) first in the test file.');
});
