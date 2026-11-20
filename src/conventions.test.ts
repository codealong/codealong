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
// How the code is read. The windows, by the tree vite's own parser makes of each file: a
// comment, a string and a label aren't code there, so none hides a use or is taken for one. The
// engine, as text, once a lexer that knows Rust's comments and strings (`rustRead`) has blanked
// the comments and strings and cut the test modules; the first tests hold it to the real files.
// Read here rather than by a cargo test, as commands.test.ts reads the engine: this takes
// seconds and needs no build, and a landing that changes code it reads runs it (land.sh).
//
// Each rule says exactly what it counts, which is the plain form of a break and no more: a
// chord kept in a constant of the window's own and a status set by a function called for it
// get past, and a reviewer is what's left. Where the plain form is wider than the rule (a
// harness named at all, where the rule is about deciding by one), the list holds today's and
// the commit that raises a number says why. These are here to stop the easy drift, not to
// prove a rule holds.
import { parseSync } from 'vite';
import { describe, expect, it } from 'vitest';

const windows = import.meta.glob<string>(['./**/*.{ts,tsx}', '!./**/*.test.{ts,tsx}', '!./testing/**'], { query: '?raw', import: 'default', eager: true });
const rust = import.meta.glob<string>(['../src-tauri/src/**/*.rs', '../src-tauri/peer/src/**/*.rs'], { query: '?raw', import: 'default', eager: true });

/** A file's name as the lists write it: from the repo's root. */
const named = (path: string) => (path.startsWith('../') ? path.slice(3) : `src/${path.slice(2)}`);

// ── The windows, as their parser reads them ─────────────────────────────────────────────────

type Node = { type: string } & Record<string, unknown>;
const isNode = (n: unknown): n is Node => !!n && typeof n === 'object' && typeof (n as Node).type === 'string';
const called = (n: unknown, name: string | Set<string>) => isNode(n) && n.type === 'Identifier' && (typeof name === 'string' ? n.name === name : name.has(n.name as string));
const KEYS = new Set(['metaKey', 'ctrlKey']);

/** Where a node keeps types, which say nothing of what runs. */
const TYPES = new Set(['typeAnnotation', 'typeArguments', 'typeParameters', 'returnType', 'superTypeArguments', 'implements']);

/** What the windows' rules count in one file, in what runs (not in its types):
 *  - `keys`: `metaKey` or `ctrlKey` read off something (`e.metaKey`, `e['ctrlKey']`) or taken
 *    out of it (`const { metaKey } = e`); not one set, on a made-up event (`{ metaKey: true }`,
 *    `init.metaKey = true`);
 *  - `chords`: a call of `chordPressed` whose chord, or some of it, is written into it;
 *  - `storage`: `localStorage` itself, read, handed on or taken out of `window`; not a
 *    property someone named so. */
function uses(file: string, source: string): { keys: number; chords: number; storage: number } {
  const { program, errors } = parseSync(file, source, { lang: file.endsWith('.tsx') ? 'tsx' : 'ts' });
  if (errors.length) throw new Error(`${file} doesn't parse: ${errors[0].message}`);
  const found = { keys: 0, chords: 0, storage: 0 };
  const walk = (n: unknown, parent?: Node, grand?: Node) => {
    if (Array.isArray(n)) return n.forEach((each) => walk(each, parent, grand));
    if (!isNode(n) || n.type === 'TSTypeAliasDeclaration' || n.type === 'TSInterfaceDeclaration') return;
    // `const { name } = …`, and `x.name = …`.
    const taken = n.type === 'Property' && parent?.type === 'ObjectPattern' && !n.computed;
    const set = parent?.type === 'AssignmentExpression' && parent.operator === '=' && parent.left === n;
    if (n.type === 'MemberExpression' && !set && (n.computed ? isNode(n.property) && KEYS.has(n.property.value as string) : called(n.property, KEYS))) found.keys++;
    if (taken && called(n.key, KEYS)) found.keys++;
    if (n.type === 'CallExpression' && (called(n.callee, 'chordPressed') || (isNode(n.callee) && called(n.callee.property, 'chordPressed')))) {
      const chord = (n.arguments as unknown[])[0];
      const written = isNode(chord) && (chord.type === 'Literal' ? typeof chord.value === 'string' : chord.type === 'TemplateLiteral' && (chord.quasis as { value: { raw: string } }[]).some((q) => q.value.raw.trim()));
      if (written) found.chords++;
    }
    if (taken && called(n.key, 'localStorage')) found.storage++;
    // The name as a key (`{ localStorage: 1 }`), or said twice by a `{ localStorage }` taken
    // out, which is counted above, isn't a use of its own.
    const key = !!parent && !parent.computed && (parent.key === n || (!!parent.shorthand && grand?.type === 'ObjectPattern'));
    if (called(n, 'localStorage') && !key) found.storage++;
    for (const k in n) if (k !== 'type' && !TYPES.has(k)) walk(n[k], n, parent);
  };
  walk(program);
  return found;
}

let read: Record<string, ReturnType<typeof uses>> | undefined;
/** Every window file's counts, parsed once. */
const inWindows = () => (read ??= Object.fromEntries(Object.entries(windows).map(([path, text]) => [named(path), uses(path, text)])));

// ── The engine, as Rust's lexer reads it ────────────────────────────────────────────────────

/** A Rust file with its comments blanked to spaces, and what its quotes hold too unless
 *  `strings`: a character for a character and line ends as they were, so a place in what this
 *  returns is the same place in the file. It knows what a text search doesn't: a `//` or a
 *  brace in a string, a raw string (`r#"…"#`) over many lines, a block comment inside another,
 *  and a quote that starts a lifetime (`'a`) rather than a character (`'a'`, `'\''`). */
function rustRead(text: string, strings: boolean): string {
  const out: string[] = [];
  let kept = 0;
  for (let i = 0; i < text.length; ) {
    const [c, next] = [text[i], text[i + 1]];
    // One past what starts here when it's a comment or quoted, and how long its quotes are.
    let [end, open, close] = [-1, 0, 0];
    if (c === '/' && next === '/') {
      end = text.indexOf('\n', i);
      if (end < 0) end = text.length;
    } else if (c === '/' && next === '*') {
      end = i + 2;
      for (let depth = 1; end < text.length && depth; ) {
        if (text.startsWith('/*', end)) [depth, end] = [depth + 1, end + 2];
        else if (text.startsWith('*/', end)) [depth, end] = [depth - 1, end + 2];
        else end++;
      }
    } else if (c === '"') {
      end = i + 1;
      while (end < text.length && text[end] !== '"') end += text[end] === '\\' ? 2 : 1;
      [end, open, close] = [Math.min(end + 1, text.length), 1, 1];
    } else if ((c === 'r' || (c === 'b' && next === 'r')) && !/\w/.test(text[i - 1] ?? '')) {
      const raw = /^b?r(#*)"/.exec(text.slice(i, i + 80));
      if (raw) {
        const closing = `"${raw[1]}`;
        const at = text.indexOf(closing, i + raw[0].length);
        [end, open, close] = at < 0 ? [text.length, raw[0].length, 0] : [at + closing.length, raw[0].length, closing.length];
      }
    } else if (c === "'") {
      if (next === '\\') end = text.indexOf("'", i + 3) + 1 || text.length;
      else if (text[i + 2] === "'") end = i + 3;
      else if (text[i + 3] === "'" && next >= '\uD800' && next <= '\uDBFF') end = i + 4;
      if (end > 0) [open, close] = [1, 1];
    }
    if (end < 0) {
      i++;
      continue;
    }
    if (c === '/' || !strings) {
      out.push(text.slice(kept, i + open), text.slice(i + open, end - close).replace(/[^\n]/g, ' '));
      kept = end - close;
    }
    i = end;
  }
  out.push(text.slice(kept));
  return out.join('');
}

/** `#[cfg(…)]` for what only tests build: `test`, or an `all(…)` with `test` among its own
 *  conditions, wherever among them (not an `any(…)`, nor a `not(test)`). */
const CFG_TEST = String.raw`#\[cfg\(\s*(?:test|all\((?:[^()]|\([^()]*\))*?\btest\b(?:[^()]|\([^()]*\))*\))\s*\)\]`;
/** A module declared: any attributes, `mod name`. Read from `rustRead`'s text with strings
 *  blanked, where a comment among them is only space and a test that quotes one isn't one. */
const MOD = String.raw`\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*`;
/** A module only tests build, with its body, and one that's a file of its own. */
const TEST_MODULE = new RegExp(`${CFG_TEST}${MOD}\\{`, 'g');
const TEST_FILE = new RegExp(`${CFG_TEST}${MOD};`, 'g');
const ANY_FILE = new RegExp(`${MOD};`, 'g');

/** A Rust file without its test modules, wherever in it they are and however deep (peer's
 *  api.rs has types after its tests): each from its `#[cfg(test)]` to the brace that closes
 *  it, which is counted in the code alone, so a brace in a string in the tests isn't one.
 *  Something else under `#[cfg(test)]` (a function for the tests, outside their module) stays,
 *  and is read as the file's own. */
function untested(rust: string): string {
  const text = rust.replace(/\r\n/g, '\n');
  const code = rustRead(text, false);
  const out: string[] = [];
  let at = 0;
  for (const opens of code.matchAll(TEST_MODULE)) {
    if (opens.index < at) continue;
    let end = opens.index + opens[0].length;
    for (let depth = 1; end < code.length && depth; end++) depth += code[end] === '{' ? 1 : code[end] === '}' ? -1 : 0;
    // Its lines go whole: the indent before it and the line end after it.
    let from = opens.index;
    while (from > at && (text[from - 1] === ' ' || text[from - 1] === '\t')) from--;
    out.push(text.slice(at, from));
    at = text[end] === '\n' ? end + 1 : end;
  }
  out.push(text.slice(at));
  return out.join('');
}

/** The files only tests build with: a module another file declares under `#[cfg(test)]`
 *  (`mod fixtures;` in board.rs is board/fixtures.rs; one with a `#[path]` is where that
 *  says), and the modules such a file declares in its turn. */
function testOnly(files: Record<string, string>): Set<string> {
  const tidy = (p: string) => {
    let was = '';
    while (was !== p) [was, p] = [p, p.replace(/\/[^/.]+\/\.\.\//, '/')];
    return p;
  };
  /** The files of the modules `pattern` finds declared in the file at `path`. */
  const declared = (path: string, pattern: RegExp) => {
    const text = files[path].replace(/\r\n/g, '\n');
    const beside = path.replace(/\/[^/]+$/, '');
    // A file's modules are beside lib.rs, main.rs and mod.rs, and in a folder of its name otherwise.
    const dir = /\/(?:lib|main|mod)\.rs$/.test(path) ? beside : path.replace(/\.rs$/, '');
    return [...rustRead(text, false).matchAll(pattern)].flatMap((mod) => {
      // Its `#[path]` is a string: read from the same place in the file as written.
      const where = /#\[path\s*=\s*"([^"]+)"\]/.exec(text.slice(mod.index, mod.index + mod[0].length))?.[1];
      return where ? [tidy(`${beside}/${where}`)] : [`${dir}/${mod[1]}.rs`, `${dir}/${mod[1]}/mod.rs`];
    });
  };
  const out = new Set<string>();
  const next = Object.keys(files).flatMap((path) => declared(path, TEST_FILE));
  for (let path = next.pop(); path; path = next.pop()) {
    if (!(path in files) || out.has(path)) continue;
    out.add(path);
    next.push(...declared(path, ANY_FILE));
  }
  return out;
}

const tests = testOnly(rust);
const engine = Object.fromEntries(Object.entries(rust).filter(([path]) => !tests.has(path)));
/** An engine file as its rules read it: only its code, with no test modules, comments and what
 *  quotes hold blanked, and each run of space one space, so how it's laid out changes nothing. */
const code = (text: string) => rustRead(untested(text), false).replace(/\s+/g, ' ');

/** The engine's module a file is part of: `src-tauri/src/board` for board.rs and board/prs.rs. */
const moduleOf = (file: string) => file.replace(/^(src-tauri\/(?:peer\/)?src\/[^/]+?)(?:\.rs|\/.*)$/, '$1');

/** A harness named: one of the enum's variants, not something of the enum's own
 *  (`Harness::ALL`, `Harness::parse`). */
const NAMED = /\bHarness::[A-Z][a-z]\w*/g;

/** A status set: an assignment to a `status` whose value names a `Status` before its statement
 *  ends, however it's worked out (`s.status = if … { Status::Unread } else { … }`). */
const STATUS = /\.status\s*=(?!=)[^;]{0,240}?\bStatus::/g;

/** The first argument of each curl the engine runs, as it's written where curl is run: the
 *  array beside `http::curl()`, or the first `.arg(…)` or `.args([…])` of the command made of
 *  it, whatever else that command is given first. Null when it isn't a string written there. */
function curlFirst(rustFile: string): (string | null)[] {
  const plain = untested(rustFile);
  const [kept, blanked] = [rustRead(plain, true), rustRead(plain, false)];
  return [...blanked.matchAll(/http::curl\(\)/g)].map((run) => {
    const from = run.index + run[0].length;
    // To the end of its statement, which a `;` in a header's string isn't.
    const end = blanked.indexOf(';', from);
    const rest = kept.slice(from, end < 0 ? undefined : end);
    const list = /^\s*,\s*&?\[\s*|\.args?\(\s*&?\[?\s*/.exec(rest);
    // A string, or a raw one: what it holds.
    const first = list && /^(?:"((?:[^"\\]|\\.)*)"|r(#*)"([\s\S]*?)"\2)/.exec(rest.slice(list.index + list[0].length));
    return first ? (first[1] ?? first[3]) : null;
  });
}

// ── What's there against what a list allows ─────────────────────────────────────────────────

/** How many each file has, for the files that have any and that aren't the rule's home. */
function counted(files: string[], home: (file: string) => boolean, count: (file: string) => number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const file of files) {
    const hits = home(file) ? 0 : count(file);
    if (hits) out[file] = hits;
  }
  return out;
}
const inEngine = (pattern: RegExp, home: (file: string) => boolean) => {
  const texts = Object.fromEntries(Object.entries(engine).map(([path, text]) => [named(path), text]));
  return counted(Object.keys(texts), home, (file) => code(texts[file]).match(pattern)?.length ?? 0);
};

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

// The parsing is the slow part on a busy Mac; this is against a hang, not a limit on a run.
describe('what the rules are read from', { timeout: 300_000 }, () => {
  it('is a window as its parser reads it: a comment, a string and a label aren’t code', () => {
    const count = (source: string) => uses('made-up.tsx', source);
    // Read, either way of writing it, and taken out of an event; not set, as a made-up event's is.
    expect(count("if (e.metaKey || e?.['ctrlKey']) go();").keys).toBe(2);
    expect(count('const { metaKey, ctrlKey: ctrl } = e;').keys).toBe(2);
    expect(count("window.dispatchEvent(new KeyboardEvent('keydown', { metaKey: true })); init.ctrlKey = true;").keys).toBe(0);
    expect(count("// e.metaKey\nconst label = 'e.ctrlKey'; /* e.metaKey */ type Mods = Pick<KeyboardEvent, 'metaKey'> & { ctrlKey: KeyboardEvent['ctrlKey'] };").keys).toBe(0);
    // A chord written into the call, or some of one, not one from a table.
    expect(count("chordPressed('Cmd+Enter', e); platform.chordPressed(`Cmd+K`, e); chordPressed(`Cmd+${key}`, e);").chords).toBe(3);
    expect(count("chordPressed(CHORDS.send, e); chordPressed(`${QUEUE_CHORDS.seen[0]}`, e); const hint = \"chordPressed('Cmd+K', e)\";").chords).toBe(0);
    // The storage itself, read, handed on or taken out of window; not the word, nor its type.
    expect(count('const kept = localStorage.getItem(k); function store() { return window.localStorage; }').storage).toBe(2);
    expect(count('const { localStorage: store } = window; function f({ localStorage }: Window) { send({ localStorage }); }').storage).toBe(3);
    expect(count("const label = 'Your browser has localStorage disabled.'; const what = { localStorage: 1 };").storage).toBe(0);
    expect(count('type Kept = Pick<typeof window.localStorage, "getItem">; const s: typeof localStorage = other as typeof localStorage;').storage).toBe(0);
    expect(count('const a = <p>Your browser has localStorage disabled.</p>;').storage).toBe(0);
    // A glob in a label isn't a comment's start: what follows it is still read.
    expect(count("const a = <p>github.com/owner/*</p>;\nfunction clear() { window.localStorage.clear(); }\nconst b = <i>{/* later */}</i>;").storage).toBe(1);
  });

  it('is Rust as its lexer reads it: comments blanked, strings known for what they are', () => {
    // A place in what's read is the same place in the file.
    const text = 'let a = "x // y"; // one\nlet b = r#"{ "k": 1 }\n}"#; /* two /* in */ it */ let c = \'}\'; let d: &\'a str = "\\"}";\n';
    const gone = (chars: number) => ' '.repeat(chars);
    expect(rustRead(text, true)).toBe(`let a = "x // y"; ${gone(6)}\nlet b = r#"{ "k": 1 }\n}"#; ${gone(21)} let c = '}'; let d: &'a str = "\\"}";\n`);
    expect(rustRead(text, false)).toBe(`let a = "${gone(6)}"; ${gone(6)}\nlet b = r#"${gone(10)}\n "#; ${gone(21)} let c = ' '; let d: &'a str = "${gone(3)}";\n`);
    expect(rustRead("let q = '\\''; let l = 'x: loop {}; let s = b\"{\"; let e = '\\u{7b}';", false)).toBe("let q = '  '; let l = 'x: loop {}; let s = b\" \"; let e = '      ';");
  });

  it('is a Rust file without its test modules, wherever they are', () => {
    const file = 'fn a() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\n\nstruct After;\n#[cfg(all(test, unix))]\nmod unix_tests {\n    fn u() {}\n}\n#[cfg(test)]\nmod fixtures;\n';
    const without = 'fn a() {}\n\n\nstruct After;\n#[cfg(test)]\nmod fixtures;\n';
    expect(untested(file)).toBe(without);
    expect(untested(file.replace(/\n/g, '\r\n'))).toBe(without);
    // A brace in a string in the tests isn't the module's end, nor one in a comment there.
    expect(untested('fn a() {}\n#[cfg(test)]\nmod tests {\n    const J: &str = r#"{\n  "a": 1\n}\n"#;\n    // }\n    fn t() {}\n}\nfn after() {}\n')).toBe('fn a() {}\nfn after() {}\n');
    // An empty one takes nothing after it, and a comment beside either end changes nothing.
    expect(untested('#[cfg(test)]\nmod tests {\n}\nfn after() {\n}\n')).toBe('fn after() {\n}\n');
    expect(untested('fn a() {}\n#[cfg(test)]\n// The tests.\n#[allow(dead_code)]\nmod tests {\n    fn t() {}\n} // tests\nstruct After;\n')).toBe('fn a() {}\n // tests\nstruct After;\n');
    // One inside another module goes too, with its indent.
    expect(untested('mod a {\n    fn f() {}\n    #[cfg(test)]\n    mod tests {\n        fn t() {}\n    }\n}\n')).toBe('mod a {\n    fn f() {}\n}\n');
  });

  it('is the engine without the files only tests build with', () => {
    const src = '../src-tauri/src';
    const files = {
      // One where its `#[path]` says, one beside lib.rs, and one only quoted, in a test.
      [`${src}/lib.rs`]: '#[cfg(test)]\n#[path = "../peer/src/fixtures.rs"]\nmod fixtures;\n// Tests only.\n#[cfg(test)]\nmod wiretypes;\nmod board;\nmod engine;\n#[cfg(not(test))]\nmod live;\n#[cfg(test)]\nmod tests {\n    const QUOTED: &str = "#[cfg(test)] mod engine;";\n}\n',
      // One in the folder of the file that declares it, whatever is beside its attribute.
      [`${src}/board.rs`]: 'mod prs;\n#[cfg(test)] // made-up sessions\nmod fixtures;\n',
      [`${src}/launch/mod.rs`]: '#[cfg(all(unix, test))]\npub(crate) mod stage;\n',
      // And what a file only tests build declares in its turn.
      [`${src}/wiretypes.rs`]: 'mod cases;\n',
      ...Object.fromEntries(['../src-tauri/peer/src/fixtures.rs', `${src}/wiretypes/cases.rs`, `${src}/board/prs.rs`, `${src}/board/fixtures.rs`, `${src}/launch/stage.rs`, `${src}/engine.rs`, `${src}/live.rs`].map((path) => [path, ''])),
    };
    expect([...testOnly(files)].sort()).toEqual(['../src-tauri/peer/src/fixtures.rs', `${src}/board/fixtures.rs`, `${src}/launch/stage.rs`, `${src}/wiretypes.rs`, `${src}/wiretypes/cases.rs`]);
  });

  it('is the real engine read right: brackets that balance, no test left in, the same whatever its line endings', () => {
    // The readers against the code itself. A string or a comment `rustRead` misreads leaves a
    // bracket open, and a test module `untested` misses leaves its tests to be counted as the
    // engine's by every rule below.
    const said: string[] = [];
    const count = (text: string, c: RegExp) => text.match(c)?.length ?? 0;
    for (const [path, text] of Object.entries(rust)) {
      const blanked = rustRead(text.replace(/\r\n/g, '\n'), false);
      const open = [/\{/g, /\(/g, /\[/g].map((c) => count(blanked, c));
      const closed = [/\}/g, /\)/g, /\]/g].map((c) => count(blanked, c));
      if (open.join() !== closed.join()) said.push(`${named(path)}: its brackets don't balance as \`rustRead\` reads it (${open.join()} opened, ${closed.join()} closed): it misreads a string or a comment there.`);
    }
    for (const [path, text] of Object.entries(engine)) {
      if (/#\[(?:\w+::)?test\b/.test(rustRead(untested(text), false))) said.push(`${named(path)} still has a test once its test modules are out: \`untested\` misreads it, or the test is outside a \`#[cfg(test)] mod\`.`);
      if (code(text.replace(/\r?\n/g, '\r\n')) !== code(text)) said.push(`${named(path)} is read differently with other line endings.`);
    }
    expect(said).toEqual([]);
  });

  it('is the engine’s code alone to a rule, laid out any way', () => {
    const names = (rustFile: string) => code(rustFile).match(NAMED)?.length ?? 0;
    // A harness named, in what runs: not in a comment, a message or a test, nor the enum's own.
    expect(names('if s.harness == Harness::Codex { Session::new(Harness::Claude, a, b) } // Harness::Cursor')).toBe(2);
    expect(names('log("h == Harness::Claude"); for h in Harness::ALL { Harness::parse(name); }')).toBe(0);
    expect(names('fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { Harness::Claude; }\n}\n')).toBe(0);
    // A status set, however far a comment pushes its value.
    expect(code(`s.status = /* ${'why '.repeat(80)} */ if live { Status::Working } else { Status::Idle }; if s.status == Status::Idle {}`).match(STATUS)?.length).toBe(1);
    // The same code on one line and on several reads the same, comments and all.
    const arms = ['match (need, h) {', '(Some(Need::Approval), Harness::Claude) => 1,', '(_, Harness::Codex | Harness::Cursor) => 2,', '_ => 3,', '}'];
    expect(code(arms.join('\n    // An arm.\n    '))).toBe(code(arms.join(' ')));
  });

  it('is the first argument curl is run with, where it’s run', () => {
    expect(curlFirst('fn a() { run(crate::http::curl(), &["-q", "-H", "Accept: a; b"], &config); }')).toEqual(['-q']);
    expect(curlFirst('fn a() {\n    run(\n        crate::http::curl(),\n        &[r"-q", "-sS"],\n    );\n}')).toEqual(['-q']);
    expect(curlFirst('fn a() { Command::new(crate::http::curl())\n    .stdin(Stdio::piped())\n    .args(["-q", "-sS"])\n    .spawn(); }')).toEqual(['-q']);
    expect(curlFirst('fn a() { Command::new(http::curl()).arg("-sS").args(["-q"]).spawn(); }')).toEqual(['-sS']);
    expect(curlFirst('fn a() { let args = ["-q"]; Command::new(http::curl()).args(args).spawn(); let b = ["-q"]; }')).toEqual([null]);
  });
});

describe('the windows', { timeout: 300_000 }, () => {
  const files = () => Object.keys(inWindows());

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
    const found = counted(files(), within('src/platform.ts'), (file) => inWindows()[file].keys);
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
    const found = counted(files(), within('src/platform.ts'), (file) => inWindows()[file].chords);
    expect(drift(found, allowed, "An action's chord goes in menus.ts `CHORDS` (a PR's in `PR_CHORDS`, the queue's in queue.ts `QUEUE_CHORDS`) and is matched by its name there; a new one also goes in platform.test.ts's list.")).toEqual([]);
  });

  it('remember things through stored.ts', () => {
    const allowed: Record<string, number> = {
      'src/drafts.ts': 1,
      'src/savedViews.ts': 2,
      'src/ui/Peek.tsx': 1,
    };
    const found = counted(files(), within('src/stored.ts'), (file) => inWindows()[file].storage);
    expect(drift(found, allowed, 'What a window remembers goes through src/stored.ts (`useStored`, `readStored`, `writeStored`), which reads a value that can be missing or unreadable and keeps each key written one way.')).toEqual([]);
  });
});

describe('the engine', { timeout: 300_000 }, () => {
  // A harness's own module, and the one that hands out the adapters; codealong-peer only
  // defines the names.
  const harnesses = (file: string) => /^src-tauri\/src\/(claude|codex|cursor|antigravity|opencode|acp|harness)$/.test(moduleOf(file)) || file.startsWith('src-tauri/peer/');

  it('asks the adapter instead of naming a harness outside its module', () => {
    // Every harness named, which is more than every decision made by one: what a branch looks
    // like depends on how it's laid out, and a name doesn't. The ones here that decide nothing
    // (a reader in board/ making its own harness's sessions) are in the list with the rest.
    const allowed: Record<string, number> = {
      'src-tauri/src/board/headlines.rs': 5,
      'src-tauri/src/board/sessions.rs': 7,
      'src-tauri/src/github.rs': 5,
      'src-tauri/src/peerfeed.rs': 1,
      'src-tauri/src/server.rs': 1,
      'src-tauri/src/start.rs': 2,
    };
    const found = inEngine(NAMED, harnesses);
    expect(drift(found, allowed, "Nothing outside a harness's module decides by which harness something is, and naming one is where that starts: ask `harness::adapter(session.harness)` or `harness::of(account)`, with a method on `Adapter` for what differs (.claude/rules/adding-a-harness.md).")).toEqual([]);
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
    const found = inEngine(STATUS, (file) => moduleOf(file) === 'src-tauri/src/board');
    expect(drift(found, allowed, "A session's status is one of the model's rules, which board.rs and board/ hold as pure functions with tests: give the rule what it needs to decide, rather than setting the status after it.")).toEqual([]);
  });

  it('keeps curl from reading ~/.curlrc wherever it is handed a key', () => {
    // A sign-in goes to curl as its config, never on a command line, and `-q`, first, keeps curl
    // from reading ~/.curlrc too, where a `trace` would write it to a file (http.rs, whose own
    // test sees the arguments as they're passed). Two of the three other places that run curl
    // had left it out. Arguments that aren't written where curl is run fail too: nothing then
    // says `-q` is first.
    const said: string[] = [];
    for (const [path, text] of Object.entries(engine)) {
      for (const first of curlFirst(text)) {
        if (first === null) said.push(`${named(path)} runs curl with arguments this can't see where it's run: write them there, \`-q\` first, so curl doesn't read ~/.curlrc (http.rs).`);
        else if (first !== '-q') said.push(`${named(path)} runs curl with ${first} first: \`-q\` comes first, so curl doesn't read ~/.curlrc (http.rs).`);
      }
    }
    expect(said).toEqual([]);
  });
});

describe('no source file outgrows what one reading can hold', { timeout: 300_000 }, () => {
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
    expect(outgrown(engine, (text) => lines(untested(text)), 2000, allowed, 'lines outside its tests', "Put what you're adding in a module of its own (a subsystem is a struct with on_* methods, as ghpoll.rs is) and call it from here.")).toEqual([]);
  });
});
