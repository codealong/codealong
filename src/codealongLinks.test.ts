import { describe, expect, it } from 'vitest';
import { BARE_SOURCE, codealongLinkOf, codealongNameOf, opened, sessionLink, sessionNamed, sessionNames, todoLink, todoNamed } from './codealongLinks';
import * as fx from './testing/fixtures';

describe('codealongLinkOf', () => {
  it('reads a session’s link and a todo’s, as they’re written', () => {
    expect(codealongLinkOf('codealong://session/claude:claude2:0e0b493b-2877')).toEqual({ kind: 'session', name: 'claude:claude2:0e0b493b-2877' });
    expect(codealongLinkOf(sessionLink('@mini:codex:codex:019a'))).toEqual({ kind: 'session', name: '@mini:codex:codex:019a' });
    expect(codealongLinkOf(todoLink('t-mw9gxqs56cwt'))).toEqual({ kind: 'todo', id: 't-mw9gxqs56cwt' });
    // However the scheme is capitalized, with a slash after it, or with more after the name.
    expect(codealongLinkOf(' Codealong://Session/claude:claude:s1/ ')).toEqual({ kind: 'session', name: 'claude:claude:s1' });
    expect(codealongLinkOf('codealong://todo/fix-login?from=notes#top')).toEqual({ kind: 'todo', id: 'fix-login' });
  });

  it('reads nothing else as one', () => {
    for (const url of ['https://github.com/a/b/pull/1', 'codealong://link/cal1abc', 'codealong://session/', 'codealong://worktree/x', 'codealong:session/x', 'files/a.txt', '']) expect(codealongLinkOf(url)).toBeNull();
  });

  it('finds a bare one in text without the punctuation after it', () => {
    const found = (text: string) => [...text.matchAll(new RegExp(BARE_SOURCE, 'g'))].map((m) => m[0]);
    expect(found('See codealong://session/claude:claude:s1, then (codealong://todo/t-1).')).toEqual(['codealong://session/claude:claude:s1', 'codealong://todo/t-1']);
    expect(found('codealong://link/cal1abc and codealong://session/')).toEqual([]);
  });
});

describe('codealongNameOf', () => {
  it('takes a key or a made id written alone, and no other code', () => {
    expect(codealongNameOf('claude:claude2:0e0b493b-2877-469f')).toEqual({ kind: 'session', name: 'claude:claude2:0e0b493b-2877-469f' });
    expect(codealongNameOf('@mini:opencode:work.acct:ses_01')).toEqual({ kind: 'session', name: '@mini:opencode:work.acct:ses_01' });
    expect(codealongNameOf('t-mw9gxqs56cwt')).toEqual({ kind: 'todo', id: 't-mw9gxqs56cwt' });
    for (const code of ['main', 'a:b', 'src/a.ts:12:4', 'codealong session show claude:claude:s1', 'http://x:80:1', 't-1', 'T-mw9gxqs56cwt', 'std::fmt::Debug', '10:42:07']) expect(codealongNameOf(code), code).toBeNull();
  });
});

describe('sessionNamed', () => {
  const a = fx.session({ key: 'claude:claude:0e0b493b-aaaa' });
  const b = fx.session({ key: 'claude:claude2:0e0b77aa-bbbb' });
  const c = fx.session({ key: 'codex:codex:019a-cccc', archived: true });
  const listed = fx.lane({ id: '/r/app', sessions: [a, b] });
  // The board's own lanes list the archived one too, and the others again.
  const own = fx.lane({ id: '/r/app', sessions: [a, b, c] });
  const names = sessionNames([listed, own]);

  it('finds one by its key or id, in the first worktree that lists it', () => {
    expect(sessionNamed(names, 'claude:claude2:0e0b77aa-bbbb')).toEqual({ lane: listed, session: b });
    expect(sessionNamed(names, '0e0b493b-aaaa')?.session).toBe(a);
    expect(sessionNamed(names, 'codex:codex:019a-cccc')).toEqual({ lane: own, session: c });
  });

  it('finds one where it lives, not in a worktree it only visited, unless that’s all that lists it', () => {
    const home = fx.session({ key: 'claude:claude:roamer1' });
    const copy = { ...home, visitor: '/r/app' };
    const guest = fx.session({ key: 'claude:claude:guest001', visitor: '/r/elsewhere' });
    const visited = fx.lane({ id: '/r/visited', sessions: [copy, guest] });
    const lived = fx.lane({ id: '/r/app', sessions: [home] });
    const roaming = sessionNames([visited, lived]);
    expect(sessionNamed(roaming, 'claude:claude:roamer1')).toEqual({ lane: lived, session: home });
    expect(sessionNamed(roaming, 'roamer')?.session).toBe(home);
    expect(sessionNamed(roaming, 'guest001')).toEqual({ lane: visited, session: guest });
  });

  it('finds one by the start of its key or id when only one starts so, as the command does', () => {
    expect(sessionNamed(names, '0e0b49')?.session).toBe(a);
    expect(sessionNamed(names, 'codex:codex:')?.session).toBe(c);
    expect(sessionNamed(names, '0e0b77aa')?.session).toBe(b);
    // Two start with it; none does; nothing is named.
    expect(sessionNamed(names, '0e0b')).toBeNull();
    expect(sessionNamed(names, 'claude:claude3:x')).toBeNull();
    expect(sessionNamed(names, '')).toBeNull();
  });
});

describe('sessionNamed, in a conversation on another Mac', () => {
  const here = fx.session({ key: 'claude:claude:abc123' });
  const there = fx.session({ key: '@mini:claude:claude:abd999', id: 'abd999' });
  const twin = fx.session({ key: '@mini:claude:claude:abc123', id: 'abc123' });
  const names = sessionNames([fx.lane({ id: '/r/app', sessions: [here] }), fx.lane({ id: '@mini:/r/app', sessions: [there, twin] })]);

  it('takes a key, an id or the start of one as that Mac’s own, and never this Mac’s', () => {
    expect(sessionNamed(names, 'claude:claude:abd999', 'mini')?.session).toBe(there);
    expect(sessionNamed(names, 'claude:claude:abc123', 'mini')?.session).toBe(twin);
    expect(sessionNamed(names, 'abc123', 'mini')?.session).toBe(twin);
    expect(sessionNamed(names, 'abc1', 'mini')?.session).toBe(twin);
    expect(sessionNamed(names, 'claude:claude:abd', 'mini')?.session).toBe(there);
    // Two there start with it; and one only this Mac has isn't that Mac's.
    expect(sessionNamed(names, 'ab', 'mini')).toBeNull();
    const alone = sessionNames([fx.lane({ id: '/r/app', sessions: [here] }), fx.lane({ id: '@mini:/r/app', sessions: [there] })]);
    expect(sessionNamed(alone, 'claude:claude:abc123', 'mini')).toBeNull();
    expect(sessionNamed(alone, 'abc1', 'mini')).toBeNull();
    // One named with its Mac is that Mac's, whichever conversation names it.
    expect(sessionNamed(names, '@mini:claude:claude:abd999', 'mini')?.session).toBe(there);
    expect(sessionNamed(names, '@mini:claude:claude:abd999')?.session).toBe(there);
  });

  it('from this Mac’s conversations names this Mac’s first, then your other Macs’, as the command does', () => {
    expect(sessionNamed(names, 'claude:claude:abc123')?.session).toBe(here);
    expect(sessionNamed(names, 'abc123')?.session).toBe(here);
    expect(sessionNamed(names, 'abc1')?.session).toBe(here);
    expect(sessionNamed(names, 'abd9')?.session).toBe(there);
    // A key of another Mac's, written without the Mac, names nothing here.
    expect(sessionNamed(names, 'claude:claude:abd999')).toBeNull();
  });
});

describe('opened', () => {
  const there = fx.session({ key: '@mini:claude:claude:abd999', id: 'abd999' });
  const here = fx.session({ key: 'claude:claude:abc123' });
  const local = sessionNames([fx.lane({ id: '/r/app', sessions: [here] })]);
  const both = sessionNames([fx.lane({ id: '/r/app', sessions: [here] }), fx.lane({ id: '@mini:/r/app', sessions: [there] })]);
  const todos = [fx.todo({ id: 't-mw9gxqs56cwt' })];

  it('names the session or the todo a link macOS opened is for', () => {
    expect(opened('codealong://session/claude:claude:abc123', local, todos)).toEqual({ session: { lane: local.get(here.key)!.lane, session: here } });
    expect(opened('codealong://session/@mini:claude:claude:abd999', both, todos)).toEqual({ session: both.get(there.key) });
    expect(opened('Codealong://todo/t-mw9gxqs', local, todos)).toEqual({ todo: todos[0] });
  });

  it('says what’s missing, and that a session may be on a device not read yet', () => {
    // Before your other devices are read, one of theirs isn't listed: it may yet be.
    expect(opened('codealong://session/@mini:claude:claude:abd999', local, todos)).toEqual({ none: 'There’s no session @mini:claude:claude:abd999 here.', elsewhere: true });
    expect(opened('codealong://todo/t-nosuch', local, todos)).toEqual({ none: 'There’s no todo t-nosuch here.', elsewhere: false });
    for (const url of ['codealong://link/cal1abc', 'codealong://session/', 'https://example.com', '']) expect(opened(url, both, todos)).toEqual({ none: 'That link names nothing Codealong shows.', elsewhere: false });
  });
});

describe('todoNamed', () => {
  const todos = [fx.todo({ id: 't-mw9gxqs56cwt' }), fx.todo({ id: 't-mw9gxql1k8gf' }), fx.todo({ id: 'fix-login' })];

  it('finds a todo by its id, or the start of only one’s', () => {
    expect(todoNamed(todos, 'fix-login')).toBe(todos[2]);
    expect(todoNamed(todos, 't-mw9gxqs')).toBe(todos[0]);
    expect(todoNamed(todos, 't-mw9gxq')).toBeNull();
    expect(todoNamed(todos, 't-zz')).toBeNull();
    expect(todoNamed(todos, '')).toBeNull();
  });
});
