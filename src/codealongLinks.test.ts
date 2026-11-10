import { describe, expect, it } from 'vitest';
import { BARE_SOURCE, codealongLinkOf, codealongNameOf, sessionLink, sessionNamed, sessionNames, todoLink, todoNamed } from './codealongLinks';
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
