// The window's commands are named twice: api.ts passes each one's name to `invoke`, and lib.rs
// lists each one's function in `generate_handler!`. Nothing else ties the two together, and a
// name on one side only shows as a rejected call in the running app, or as a command nobody can
// reach. This reads both files as text and checks that they name the same commands.
import { describe, expect, it } from 'vitest';
import lib from '../src-tauri/src/lib.rs?raw';

// api.ts is the only file that calls `invoke`; every source file is read anyway, so a call
// added somewhere else is checked too.
const sources = import.meta.glob<string>(['./**/*.{ts,tsx}', '!./**/*.test.{ts,tsx}'], { query: '?raw', import: 'default', eager: true });

/** The commands lib.rs registers: the names in its one `generate_handler![…]`, a path's last part. */
function registered(rust: string): string[] {
  const lists = [...rust.matchAll(/generate_handler!\s*\[([^\]]*)\]/g)];
  if (lists.length !== 1) throw new Error(`expected one generate_handler![…] in lib.rs, found ${lists.length}`);
  return lists[0][1]
    .replace(/\/\/.*$/gm, '')
    .split(',')
    .map((name) => name.trim().split('::').pop() ?? '')
    .filter(Boolean);
}

/** A call of `invoke`, with or without a type for what it returns, and the name it's given when
 *  that's a plain string. */
const CALL = /\binvoke\s*(?:<[^()]*>)?\s*\(\s*(?:(['"])([^'"]*)\1)?/g;

/** Each command a file invokes, and the calls whose command isn't a plain string, by line. */
function invoked(text: string): { names: string[]; unnamed: number[] } {
  const names: string[] = [];
  const unnamed: number[] = [];
  for (const m of text.matchAll(CALL)) {
    if (m[2] === undefined) unnamed.push(text.slice(0, m.index).split('\n').length);
    // A plugin's commands are its own to register.
    else if (!m[2].startsWith('plugin:')) names.push(m[2]);
  }
  return { names, unnamed };
}

describe('the commands the window invokes', () => {
  const handlers = new Set(registered(lib));
  const callers = new Map<string, string>();
  const unreadable: string[] = [];
  for (const [file, text] of Object.entries(sources)) {
    const { names, unnamed } = invoked(text);
    for (const name of names) if (!callers.has(name)) callers.set(name, file);
    for (const line of unnamed) unreadable.push(`${file}:${line}`);
    // A call written some way CALL doesn't follow would otherwise go unchecked.
    const calls = text.match(/\binvoke\s*[<(]/g)?.length ?? 0;
    if (calls !== names.length + unnamed.length) unreadable.push(`${file}: ${calls} calls of invoke, ${names.length + unnamed.length} read`);
  }

  it('finds both lists', () => {
    expect(handlers.size).toBeGreaterThan(0);
    expect(callers.size).toBeGreaterThan(0);
  });

  it('are each named by a plain string', () => {
    expect(unreadable, `invoke calls this test can't read the command's name from: ${unreadable.join(', ')}`).toEqual([]);
  });

  it('are the ones lib.rs registers', () => {
    // Both sides at once, so one run says everything that's out of step.
    const unregistered = [...callers].filter(([name]) => !handlers.has(name)).map(([name, file]) => `${name} is invoked in ${file} but isn't in generate_handler! in src-tauri/src/lib.rs`);
    const uncalled = [...handlers].filter((name) => !callers.has(name)).map((name) => `${name} is in generate_handler! in src-tauri/src/lib.rs but nothing in src invokes it`);
    const apart = [...unregistered, ...uncalled];
    expect(apart, apart.join('\n')).toEqual([]);
  });
});
