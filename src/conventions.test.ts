// CLAUDE.md's conventions that a search can check, checked: each rule here is a pattern, where
// it's at home, and the places that break it today, counted. A rule that's only written down
// drifts while many agents work at once, each reading a few screens of a file; the ones a test
// held (the commands, the styles' classes, `cfg(target_os)` in os/) had no breaks when the code
// was audited on 2026-11-19, and the ones below had the counts listed.
//
// The lists only go down. A file with more of a pattern than its line allows, or a new file
// with any, fails, and the failure says what to do instead. A file with fewer fails too, asking
// for its number to be lowered, so the room made isn't used by the next change. Raising a number
// is for a use the rule doesn't fit, with the reason in the commit: a reviewer sees the diff.
//
// The engine's files are read here as text, as commands.test.ts reads them, rather than by a
// cargo test: this takes seconds and needs no build, so it's run on every landing.
import { describe, expect, it } from 'vitest';

const windows = import.meta.glob<string>(['./**/*.{ts,tsx}', '!./**/*.test.{ts,tsx}', '!./testing/**'], { query: '?raw', import: 'default', eager: true });
const engine = import.meta.glob<string>(['../src-tauri/src/**/*.rs', '../src-tauri/peer/src/**/*.rs'], { query: '?raw', import: 'default', eager: true });

/** A file's name as the lists write it: from the repo's root. */
const named = (path: string) => (path.startsWith('../') ? path.slice(3) : `src/${path.slice(2)}`);

/** A Rust file up to its tests: everything before its `#[cfg(test)] mod tests`. A module only
 *  tests use that's named near the top (board.rs's `mod fixtures;`) isn't where they begin. */
function untested(rust: string): string {
  const tests = rust.search(/^#\[cfg\(test\)\]\s*\n(?:pub(?:\([a-z]+\))? )?mod tests\b/m);
  return tests < 0 ? rust : rust.slice(0, tests);
}

/** A Rust file without its tests and without its comments, which name the things the rules are
 *  about. */
function code(rust: string): string {
  return untested(rust)
    .split('\n')
    .filter((line) => !/^\s*\/\//.test(line))
    .join('\n');
}

/** A TypeScript file without its line comments. */
const spoken = (ts: string) =>
  ts
    .split('\n')
    .filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line))
    .join('\n');

/** The engine's module a file is part of, which its rules count by: `src-tauri/src/board` for
 *  board.rs and for board/prs.rs, so a file split along its seams keeps its line in a list. */
const moduleOf = (file: string) => file.replace(/^(src-tauri\/(?:peer\/)?src\/[^/]+?)(?:\.rs|\/.*)$/, '$1');

/** How often `pattern` is in each file (or each of `by`'s groups of them), for those it's in at
 *  all and that aren't its home. */
function counted(files: Record<string, string>, read: (text: string) => string, pattern: RegExp, home: (name: string) => boolean, by: (file: string) => string = (file) => file): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [path, text] of Object.entries(files)) {
    const name = by(named(path));
    if (home(name)) continue;
    const hits = read(text).match(pattern)?.length ?? 0;
    if (hits) out[name] = (out[name] ?? 0) + hits;
  }
  return out;
}

/** What's wrong between what's there and what the list allows, a line for each file. */
function drift(found: Record<string, number>, allowed: Record<string, number>, instead: string): string[] {
  const said: string[] = [];
  for (const file of [...new Set([...Object.keys(found), ...Object.keys(allowed)])].sort()) {
    const [has, may] = [found[file] ?? 0, allowed[file] ?? 0];
    if (has > may) said.push(`${file} has ${has}, and the list allows ${may}. ${instead}`);
    else if (has < may) said.push(`${file} has ${has} now, fewer than the ${may} the list allows: lower its number in src/conventions.test.ts${has ? '' : ' (take its line out)'}.`);
  }
  return said;
}

const within = (...homes: string[]) => (file: string) => homes.some((h) => (h.endsWith('/') ? file.startsWith(h) : file === h));

describe('the windows', () => {
  it('match keys through platform.ts, not by reading metaKey or ctrlKey', () => {
    // The shortcut recorder and the terminal's own keys read what was pressed as it was pressed;
    // the rest are from before the rule was held.
    const allowed: Record<string, number> = {
      'src/keysShown.ts': 2,
      'src/quick.ts': 18,
      'src/typingTrace.ts': 2,
      'src/ui/terminal/controller.ts': 1,
      'src/ui/terminal/keys.ts': 6,
    };
    const found = counted(windows, spoken, /\.(?:metaKey|ctrlKey)\b/g, within('src/platform.ts'));
    expect(drift(found, allowed, 'A key is matched with chordPressed, held or cmd (src/platform.ts), which press it as the platform has it: ⌘ on macOS is Ctrl on Linux.')).toEqual([]);
  });

  it('keep a chord in a table, not in the call that matches it', () => {
    // A chord written into `chordPressed('Cmd+Enter', e)` is in one window only: the menus, the
    // strip and the other windows don't show it or press it, and platform.test.ts's list of the
    // chords that must stay apart on Linux doesn't know it.
    const allowed: Record<string, number> = {
      'src/history.ts': 2,
      'src/mainKeys.ts': 5,
      'src/ui/EditFrom.tsx': 1,
      'src/ui/PrBox.tsx': 2,
      'src/ui/QuestionList.tsx': 1,
      'src/ui/Quick.tsx': 1,
      'src/ui/SessionReply.tsx': 5,
    };
    const found = counted(windows, spoken, /\bchordPressed\(\s*['"`]/g, within('src/platform.ts'));
    expect(drift(found, allowed, "An action's chord goes in menus.ts `CHORDS` (a PR's in `PR_CHORDS`, the queue's in queue.ts `QUEUE_CHORDS`) and is matched by its name there; a new one also goes in platform.test.ts's list.")).toEqual([]);
  });

  it('remember things through stored.ts', () => {
    const allowed: Record<string, number> = {
      'src/drafts.ts': 1,
      'src/savedViews.ts': 2,
      'src/ui/Peek.tsx': 1,
    };
    const found = counted(windows, spoken, /\blocalStorage\b/g, within('src/stored.ts'));
    expect(drift(found, allowed, 'What a window remembers goes through src/stored.ts (`useStored`, `stored`), which reads a value that can be missing or unreadable and keeps each key written one way.')).toEqual([]);
  });
});

describe('the engine', () => {
  // Counted by module (`moduleOf`). A harness's own module, and the one that hands out the
  // adapters; codealong-peer only defines the names.
  const harnesses = (module: string) => /^src-tauri\/src\/(claude|codex|cursor|antigravity|opencode|acp|harness)$/.test(module) || module.startsWith('src-tauri/peer/');

  it('asks the adapter instead of naming a harness outside its module', () => {
    const allowed: Record<string, number> = {
      'src-tauri/src/board': 12,
      'src-tauri/src/github': 5,
      'src-tauri/src/peerfeed': 1,
      'src-tauri/src/server': 1,
      'src-tauri/src/start': 2,
    };
    const found = counted(engine, code, /\bHarness::[A-Z]\w+/g, harnesses, moduleOf);
    expect(drift(found, allowed, "Nothing outside a harness's module branches on which harness it is: ask `harness::adapter(session.harness)` or `harness::of(account)`, with a method on `Adapter` for what differs (.claude/rules/adding-a-harness.md).")).toEqual([]);
  });

  it('sets a session’s status in board.rs', () => {
    const allowed: Record<string, number> = {
      'src-tauri/src/acp': 3,
      'src-tauri/src/claude': 1,
      'src-tauri/src/engine': 1,
      'src-tauri/src/starting': 1,
      'src-tauri/src/terminal': 4,
    };
    const found = counted(engine, code, /\.status = Status::/g, within('src-tauri/src/board'), moduleOf);
    expect(drift(found, allowed, "A session's status is one of the model's rules, which board.rs holds as pure functions with tests: give the rule what it needs to decide, rather than setting the status after it.")).toEqual([]);
  });

  it('starts work it waits on through throttle::background', () => {
    const allowed: Record<string, number> = {
      'src-tauri/peer/src/client': 5,
      'src-tauri/src/acp': 1,
      'src-tauri/src/actions': 2,
      'src-tauri/src/agentdone': 2,
      'src-tauri/src/antigravity': 1,
      'src-tauri/src/bar': 4,
      'src-tauri/src/cli': 2,
      'src-tauri/src/desktop': 2,
      'src-tauri/src/dictation': 4,
      'src-tauri/src/engine': 8,
      'src-tauri/src/flash': 1,
      'src-tauri/src/frames': 1,
      'src-tauri/src/github': 6,
      'src-tauri/src/held': 1,
      'src-tauri/src/hibernate': 1,
      'src-tauri/src/hygiene': 1,
      'src-tauri/src/improve': 2,
      'src-tauri/src/install': 1,
      'src-tauri/src/jev': 1,
      'src-tauri/src/kept': 3,
      'src-tauri/src/launch': 2,
      'src-tauri/src/ledger': 2,
      'src-tauri/src/lib': 9,
      'src-tauri/src/lineage': 3,
      'src-tauri/src/listen': 2,
      'src-tauri/src/meetings': 1,
      'src-tauri/src/menubar': 3,
      'src-tauri/src/os': 5,
      'src-tauri/src/outbox': 2,
      'src-tauri/src/panel': 2,
      'src-tauri/src/peek': 1,
      'src-tauri/src/peers': 14,
      'src-tauri/src/perf': 1,
      'src-tauri/src/previews': 1,
      'src-tauri/src/procs': 1,
      'src-tauri/src/quick': 1,
      'src-tauri/src/reviewdone': 1,
      'src-tauri/src/rpc': 3,
      'src-tauri/src/schedules': 1,
      'src-tauri/src/search': 1,
      'src-tauri/src/sentfiles': 1,
      'src-tauri/src/serve': 11,
      'src-tauri/src/server': 7,
      'src-tauri/src/side': 2,
      'src-tauri/src/signin': 1,
      'src-tauri/src/stance': 1,
      'src-tauri/src/terminal': 1,
      'src-tauri/src/terminals': 5,
      'src-tauri/src/updates': 4,
      'src-tauri/src/util': 1,
      'src-tauri/src/view': 2,
      'src-tauri/src/zoom': 1,
    };
    const found = counted(engine, code, /\bthread::(?:spawn|scope)\b|\bthread::Builder\b/g, within('src-tauri/src/throttle'), moduleOf);
    expect(drift(found, allowed, 'Work the engine waits on from another thread goes through `throttle::background`, so its flag comes off however the work ends (a panic too). A thread that lives as long as the process, or that nothing waits on, is a use the rule leaves: raise the number and say which it is.')).toEqual([]);
  });

  it('keeps curl from reading ~/.curlrc wherever it is handed a key', () => {
    // A sign-in goes to curl as its config, never on a command line, and `-q`, first, keeps curl
    // from reading ~/.curlrc too, where a `trace` would write it to a file (http.rs). Two of the
    // three places that run curl outside http.rs had left it out.
    const said: string[] = [];
    for (const [path, text] of Object.entries(engine)) {
      for (const run of code(text).matchAll(/http::curl\(\)[\s\S]{0,160}?\[\s*"([^"]*)"/g)) {
        if (run[1] !== '-q') said.push(`${named(path)} runs curl with ${run[1]} first: \`-q\` comes first, so curl doesn't read ~/.curlrc (http.rs).`);
      }
    }
    expect(said).toEqual([]);
  });
});

describe('no file outgrows what one reading can hold', () => {
  // An agent reads a few hundred lines around what it searched for. A file far longer than that
  // is edited without the rule three screens away being seen, and is in every other session's
  // rebase. New code goes in a file of its own; these are the files already over the limit, each
  // at the size it had when its line was written and fifty to a hundred and fifty lines more
  // (the next hundred past fifty), so that a call to something new still fits. Their numbers
  // come down as they're split: a file a hundred and fifty lines under its limit is asked to
  // lower it.
  const lines = (text: string) => text.split('\n').length;

  it('in the windows: 1,200 lines', () => {
    const allowed: Record<string, number> = {
      'src/App.tsx': 3200,
      'src/ahead.ts': 1400,
      'src/api.ts': 2600,
      'src/boardLists.ts': 1500,
      'src/menus.ts': 2000,
      'src/ui/Bar.tsx': 1300,
      'src/ui/Quick.tsx': 3300,
      'src/ui/SessionReply.tsx': 2000,
      'src/ui/SessionThread.tsx': 1600,
      'src/ui/SettingsDialog.tsx': 3100,
    };
    const said: string[] = [];
    for (const [path, text] of Object.entries(windows)) {
      const file = named(path);
      const [has, may] = [lines(text), allowed[file] ?? 1200];
      if (has > may) said.push(`${file} is ${has} lines, over its ${may}. Put what you're adding in a file of its own (a hook, a component, pure functions with a test) and call it from here.`);
      else if (allowed[file] && has <= may - 150) said.push(`${file} is ${has} lines now: lower its limit in src/conventions.test.ts to ${Math.ceil((has + 50) / 100) * 100}${has <= 1200 ? ' (take its line out)' : ''}.`);
    }
    expect(said.sort()).toEqual([]);
  });

  it('in the engine: 2,000 lines outside a file’s tests', () => {
    const allowed: Record<string, number> = {
      'src-tauri/src/actions.rs': 2500,
      'src-tauri/src/antigravity.rs': 2400,
      'src-tauri/src/autoreview.rs': 3400,
      'src-tauri/src/claude/mod.rs': 2400,
      'src-tauri/src/cli.rs': 2200,
      'src-tauri/src/config.rs': 3600,
      'src-tauri/src/cursor.rs': 2800,
      'src-tauri/src/engine.rs': 5400,
      'src-tauri/src/github.rs': 2400,
    };
    const said: string[] = [];
    for (const [path, text] of Object.entries(engine)) {
      const file = named(path);
      const [has, may] = [lines(untested(text)), allowed[file] ?? 2000];
      if (has > may) said.push(`${file} is ${has} lines outside its tests, over its ${may}. Put what you're adding in a module of its own (a subsystem is a struct with on_* methods, as ghpoll.rs is) and call it from here.`);
      else if (allowed[file] && has <= may - 150) said.push(`${file} is ${has} lines outside its tests now: lower its limit in src/conventions.test.ts to ${Math.ceil((has + 50) / 100) * 100}${has <= 2000 ? ' (take its line out)' : ''}.`);
    }
    expect(said.sort()).toEqual([]);
  });
});
