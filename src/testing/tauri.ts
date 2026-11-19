// The engine, as a unit test has it: what stands in for Tauri under src/api.ts, the only place
// the windows talk to it. The code under test runs its real api.ts, and @tauri-apps/api's own
// `invoke` and `listen`, over this: a command is recorded and answered as the test says, and an
// event the test emits reaches whoever in the page listens to it. scripts/preview/fake-tauri.js
// does the same for the browser checks, with a board snapshotted from real data behind it; this
// one has nothing behind it but what a test puts there.
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

if (typeof window === 'undefined') throw new Error('src/testing/tauri.ts needs a page: start the test file with `// @vitest-environment jsdom`.');

const calls: Call[] = [];
const answers = new Map<string, Answer>();
/** What a command answers when a test says nothing else, where null isn't an answer: the reads
 *  the menus make by themselves, the accounts as menus.ts loads, before a test could answer, and
 *  the start options for a session that isn't running (startAccounts.ts). None of either. */
const STANDING: Record<string, unknown> = { get_accounts: [], start_options: { accounts: [], codexProjects: [], claudeModels: [] } };
// Tauri's callbacks, by the id `transformCallback` gave each, and who listens to which event.
const callbacks = new Map<number, (data: unknown) => void>();
const listeners = new Map<number, { event: string; handler: number }>();
let ids = 0;
/** This window's label, as `getCurrentWebviewWindow()` reads it. */
const LABEL = 'main';

const EVENTS = 'plugin:event|';

/** What Tauri's `invoke` comes to here. (Not named for it: commands.test.ts reads every call of
 *  one in src for the command it names.) */
function asked(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const listens = cmd === `${EVENTS}listen`;
  const unlistens = cmd === `${EVENTS}unlisten`;
  if (!listens && !unlistens) calls.push({ cmd, args });
  // Answered later than it's asked, as the engine answers: what the page does next doesn't have
  // the answer yet, and an event emitted before a listener is in doesn't reach it.
  return Promise.resolve().then(() => {
    if (listens) {
      const id = ++ids;
      listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
      return id;
    }
    if (unlistens) {
      listeners.delete(Number(args.eventId));
      return null;
    }
    const answer = answers.get(cmd);
    return answer ? answer(args) : (STANDING[cmd] ?? null);
  });
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
    metadata: { currentWindow: { label: LABEL }, currentWebview: { windowLabel: LABEL, label: LABEL } },
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
  /** What `cmd` answers from now until the test ends: a value, or what a function makes of the
   *  arguments (a promise the test settles, a throw for one the engine turns down). A command
   *  with no answer resolves null, which is right for the many that only do something, but for
   *  the few in `STANDING`. */
  answer: (cmd: string, answer: unknown): void => void answers.set(cmd, typeof answer === 'function' ? (answer as Answer) : () => answer),
  /** Emits an event to the page as the engine does: everyone listening to it hears it, in the
   *  order they began to. */
  emit: (event: string, payload: unknown): void => {
    for (const [id, listener] of [...listeners]) {
      if (listener.event === event) callbacks.get(listener.handler)?.({ event, id, payload });
    }
  },
  /** How many in the page listen to an event now. */
  listening: (event: string): number => [...listeners.values()].filter((l) => l.event === event && callbacks.has(l.handler)).length,
};

// Each test starts with nothing sent and nothing answered. Who listens is the page's own to end.
afterEach(() => {
  calls.length = 0;
  answers.clear();
});

beforeAll(async () => {
  const { inTauri } = await import('../api');
  if (!inTauri) throw new Error('src/api.ts loaded before src/testing/tauri.ts, so it took the page for a browser preview: import ./testing/tauri (or ./testing/dom) first in the test file.');
});
