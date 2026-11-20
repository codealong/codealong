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
// A search finds the plain form of a break and no more: the key read off the event, the chord
// in the call, the status assigned. Written another way (a key destructured, a chord kept in a
// constant of the window's own, arguments built up in a variable) it gets past, and a reviewer
// is what's left. These are here to stop the easy drift, not to prove the rule holds.
//
// The engine's files are read here as text, as commands.test.ts reads them, rather than by a
// cargo test: this takes seconds and needs no build, so it's run on every landing.
import { describe, expect, it } from 'vitest';

const windows = import.meta.glob<string>(['./**/*.{ts,tsx}', '!./**/*.test.{ts,tsx}', '!./testing/**'], { query: '?raw', import: 'default', eager: true });
// Not the files only tests build with (`fixtures.rs`, named by a `#[cfg(test)] mod` elsewhere).
const engine = import.meta.glob<string>(['../src-tauri/src/**/*.rs', '../src-tauri/peer/src/**/*.rs', '!../src-tauri/**/fixtures.rs'], { query: '?raw', import: 'default', eager: true });

/** A file's name as the lists write it: from the repo's root. */
const named = (path: string) => (path.startsWith('../') ? path.slice(3) : `src/${path.slice(2)}`);

/** Source without its comments, which name the things the rules are about: block comments and
 *  `//` to the end of its line, wherever on a line they are. Strings are left in, so a rule's
 *  pattern is one a string wouldn't hold by chance (a member read, a call, an assignment). */
function bare(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
}

/** A Rust file without its test modules: each `#[cfg(test)] mod … { … }`, up to the brace
 *  that closes it at the start of a line, wherever in the file it is (peer/src/api.rs has
 *  types after its tests). A module only named there (`#[cfg(test)] mod fixtures;`) is a line. */
function untested(rust: string): string {
  return rust.replace(/^#\[cfg\((?:all\()?test\b[^\]]*\]\s*\n(?:pub(?:\([a-z]+\))? )?mod \w+ \{\n[\s\S]*?\n\}\n?/gm, '');
}

/** The engine's module a file is part of: `src-tauri/src/board` for board.rs and board/prs.rs. */
const moduleOf = (file: string) => file.replace(/^(src-tauri\/(?:peer\/)?src\/[^/]+?)(?:\.rs|\/.*)$/, '$1');

/** How often `pattern` is in each file, for the files it's in at all and that aren't its home. */
function counted(files: Record<string, string>, read: (text: string) => string, pattern: RegExp, home: (file: string) => boolean): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [path, text] of Object.entries(files)) {
    const file = named(path);
    if (home(file)) continue;
    const hits = read(text).match(pattern)?.length ?? 0;
    if (hits) out[file] = hits;
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

describe('what the rules are read from', () => {
  it('is the code, without its comments and its tests', () => {
    // A comment that names a thing isn't a use of it, and one before a use doesn't hide it.
    expect(bare('/* localStorage */ localStorage.getItem(k); // e.metaKey\nconst a = 1; /* two\nlines */ b.ctrlKey')).toBe(' localStorage.getItem(k); \nconst a = 1;  b.ctrlKey');
    // A test module goes wherever it is, and what comes after it stays.
    const rust = 'fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\nstruct After;\n#[cfg(all(test, unix))]\nmod unix_tests {\n    fn u() {}\n}\n#[cfg(test)]\nmod fixtures;\n';
    expect(untested(rust)).toBe('fn a() {}\nstruct After;\n#[cfg(test)]\nmod fixtures;\n');
  });
});

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
    const found = counted(windows, bare, /\.(?:metaKey|ctrlKey)\b/g, within('src/platform.ts'));
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
    const found = counted(windows, bare, /\bchordPressed\(\s*['"`]/g, within('src/platform.ts'));
    expect(drift(found, allowed, "An action's chord goes in menus.ts `CHORDS` (a PR's in `PR_CHORDS`, the queue's in queue.ts `QUEUE_CHORDS`) and is matched by its name there; a new one also goes in platform.test.ts's list.")).toEqual([]);
  });

  it('remember things through stored.ts', () => {
    const allowed: Record<string, number> = {
      'src/savedViews.ts': 2,
      'src/ui/Peek.tsx': 1,
    };
    const found = counted(windows, bare, /\blocalStorage\s*[.[]/g, within('src/stored.ts'));
    expect(drift(found, allowed, 'What a window remembers goes through src/stored.ts (`useStored`, `readStored`, `writeStored`), which reads a value that can be missing or unreadable and keeps each key written one way.')).toEqual([]);
  });
});

describe('the engine', () => {
  // A harness's own module, and the one that hands out the adapters; codealong-peer only
  // defines the names.
  const harnesses = (file: string) => /^src-tauri\/src\/(claude|codex|cursor|antigravity|opencode|acp|harness)$/.test(moduleOf(file)) || file.startsWith('src-tauri/peer/');

  it('asks the adapter instead of branching on a harness outside its module', () => {
    // Where the code decides by which harness it is: a comparison, a `matches!`, a match arm.
    // Naming one to make a session of it (`Session::new(Harness::Claude, …)`) decides nothing.
    const branch = /(?:[=!]=\s*|\bmatches!\([^;]*?)Harness::[A-Z]\w+|Harness::[A-Z]\w+(?:\s*\|\s*Harness::[A-Z]\w+)*\s*=>/g;
    const allowed: Record<string, number> = {
      'src-tauri/src/peerfeed.rs': 1,
      'src-tauri/src/start.rs': 1,
    };
    const found = counted(engine, (rust) => bare(untested(rust)), branch, harnesses);
    expect(drift(found, allowed, "Nothing outside a harness's module branches on which harness it is: ask `harness::adapter(session.harness)` or `harness::of(account)`, with a method on `Adapter` for what differs (.claude/rules/adding-a-harness.md).")).toEqual([]);
  });

  it('sets a session’s status in board’s rules', () => {
    // An assignment to a `status` whose value names a `Status`, however it's worked out
    // (`s.status = if … { Status::Unread } else { … }`).
    const allowed: Record<string, number> = {
      'src-tauri/src/acp.rs': 5,
      'src-tauri/src/claude/asks.rs': 1,
      'src-tauri/src/engine.rs': 1,
      'src-tauri/src/starting.rs': 1,
      'src-tauri/src/terminal.rs': 4,
    };
    const found = counted(engine, (rust) => bare(untested(rust)), /\.status\s*=(?!=)[^;]{0,240}?\bStatus::/g, (file) => moduleOf(file) === 'src-tauri/src/board');
    expect(drift(found, allowed, "A session's status is one of the model's rules, which board.rs and board/ hold as pure functions with tests: give the rule what it needs to decide, rather than setting the status after it.")).toEqual([]);
  });

  it('keeps curl from reading ~/.curlrc wherever it is handed a key', () => {
    // A sign-in goes to curl as its config, never on a command line, and `-q`, first, keeps curl
    // from reading ~/.curlrc too, where a `trace` would write it to a file (http.rs, whose own
    // test sees the arguments as they're passed). Two of the three other places that run curl
    // had left it out: this sees the first argument written after `http::curl()`.
    const said: string[] = [];
    for (const [path, text] of Object.entries(engine)) {
      for (const run of bare(untested(text)).matchAll(/http::curl\(\)[\s\S]{0,160}?\[\s*"([^"]*)"/g)) {
        if (run[1] !== '-q') said.push(`${named(path)} runs curl with ${run[1]} first: \`-q\` comes first, so curl doesn't read ~/.curlrc (http.rs).`);
      }
    }
    expect(said).toEqual([]);
  });
});

describe('no source file outgrows what one reading can hold', () => {
  // An agent reads a few hundred lines around what it searched for. A file far longer than that
  // is edited without the rule three screens away being seen, and is in every other session's
  // rebase. New code goes in a file of its own; these are the files already over the limit, each
  // at the size it had when its line was written and fifty to a hundred and fifty lines more
  // (the next hundred past fifty), so that a call to something new still fits. Their numbers
  // come down as they're split: a file a hundred and fifty lines under its limit is asked to
  // lower it. The windows' .ts and .tsx outside their tests, and the engine's .rs outside its.
  const lines = (text: string) => text.split('\n').length - (text.endsWith('\n') ? 1 : 0);

  /** What's wrong with the lengths of `files` against `limit` and the list of those past it. */
  function outgrown(files: Record<string, string>, length: (text: string) => number, limit: number, allowed: Record<string, number>, what: string, instead: string): string[] {
    const said: string[] = [];
    const there = new Set<string>();
    for (const [path, text] of Object.entries(files)) {
      const file = named(path);
      there.add(file);
      const [has, may] = [length(text), allowed[file] ?? limit];
      if (has > may) said.push(`${file} is ${has} ${what}, over its ${may}. ${instead}`);
      else if (allowed[file] && has <= may - 150) said.push(`${file} is ${has} ${what} now: lower its limit in src/conventions.test.ts to ${Math.ceil((has + 50) / 100) * 100}${has <= limit ? ' (take its line out)' : ''}.`);
    }
    // A line for a file that's gone would be the next file's of that name to grow into.
    for (const file of Object.keys(allowed)) if (!there.has(file)) said.push(`${file} isn't there any more: take its line out of src/conventions.test.ts.`);
    return said.sort();
  }

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
    expect(outgrown(windows, lines, 1200, allowed, 'lines', "Put what you're adding in a file of its own (a hook, a component, pure functions with a test) and call it from here.")).toEqual([]);
  });

  it('in the engine: 2,000 lines outside a file’s tests', () => {
    const allowed: Record<string, number> = {
      'src-tauri/src/actions.rs': 2500,
      'src-tauri/src/antigravity.rs': 2400,
      'src-tauri/src/autoreview.rs': 3400,
      'src-tauri/src/claude/mod.rs': 2400,
      'src-tauri/src/cli.rs': 2200,
      'src-tauri/src/config.rs': 3600,
      'src-tauri/src/cursor.rs': 2700,
      'src-tauri/src/engine.rs': 5400,
      'src-tauri/src/github.rs': 2400,
    };
    expect(outgrown(engine, (rust) => lines(untested(rust)), 2000, allowed, 'lines outside its tests', "Put what you're adding in a module of its own (a subsystem is a struct with on_* methods, as ghpoll.rs is) and call it from here.")).toEqual([]);
  });
});
