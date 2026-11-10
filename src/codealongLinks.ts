// Links to what Codealong shows, as an agent writes them in a reply (the codealong skill says
// how): `[its title](codealong://session/<key>)` for a session and `codealong://todo/<id>` for a
// todo. Which session or todo a link names, how one is written, and the sessions a window lists
// by what a link may name one by, as the command does (cli.rs `named`): its key or id, or the
// start of only one's. The command takes the same links where it takes a key or an id (cli.rs
// `named_by`). A link only ever shows what it names: nothing here sends, answers, stops or
// starts.

import { sessionHomes, type Placed } from './lanes';
import type { Lane, Todo } from './types';

const SCHEME = /^codealong:\/\/(session|todo)\/([^?#\s]+)/i;

/** A bare link in text, without the punctuation that ends a sentence around it. */
export const BARE_SOURCE = String.raw`codealong:\/\/(?:session|todo)\/[^\s<>()\`]*[^\s<>()\`.,;:!?'"*_]`;

/** A session's key as the board writes one, alone: harness, account and id, after its device's
 *  short name for one on another of your Macs. */
const KEY_SHAPE = /^(?:@[\w-]+:)?[a-z][\w-]*:[\w.-]+:[\w.-]+$/;
/** A todo's id as Codealong makes one, alone. */
const TODO_SHAPE = /^t-[a-z0-9]{8,}$/;

/** What a piece of code names when it's a session's key or a todo's id and nothing else, as an
 *  agent writes one it got from the command; null for any other code. Whether there's such a
 *  session or todo is the board's to say. */
export function codealongNameOf(code: string): CodealongLink | null {
  if (KEY_SHAPE.test(code)) return { kind: 'session', name: code };
  if (TODO_SHAPE.test(code)) return { kind: 'todo', id: code };
  return null;
}

export type CodealongLink = { kind: 'session'; name: string } | { kind: 'todo'; id: string };

/** What a link goes to; null for any other link, and for one of Codealong's that names nothing a
 *  window shows (a device's link code, `codealong://link/…`). */
export function codealongLinkOf(url: string): CodealongLink | null {
  const m = SCHEME.exec(url.trim());
  if (!m) return null;
  const named = m[2].replace(/\/+$/, '');
  if (!named) return null;
  return m[1].toLowerCase() === 'session' ? { kind: 'session', name: named } : { kind: 'todo', id: named };
}

/** A session's link, by its key. */
export const sessionLink = (key: string) => `codealong://session/${key}`;

/** A todo's link. */
export const todoLink = (id: string) => `codealong://todo/${id}`;

/** A window's sessions by key, each where it lives (lanes.ts `Placed`). */
export type SessionNames = Map<string, Placed>;

/** The sessions of `lanes`, each once: under the worktree it's listed in as its own, the first
 *  of them (lanes.ts `sessionHomes`), and one listed only where it visits, there, as the
 *  command names one (cli.rs `all_in`). A worktree a session only looked into lists a copy of it
 *  that says what it did there, not what it's doing. */
export function sessionNames(lanes: Lane[]): SessionNames {
  const names = sessionHomes(lanes);
  for (const lane of lanes) for (const session of lane.sessions) if (!names.has(session.key)) names.set(session.key, { lane, session });
  return names;
}

/** The one of a device's sessions that `name` names, as the command names one (cli.rs `named`):
 *  the one whose key or id it is, else the only one whose key or id starts with it, or whose
 *  short id it is. `device` is what its keys start with: `@<short>:` for another of your Macs
 *  (`@` for any of them), nothing for this one. Null when more than one is named, and undefined
 *  when none is. */
function namedOn(names: SessionNames, name: string, device: string): Placed | null | undefined {
  const mine = (key: string) => (device ? key.startsWith(device) : !key.startsWith('@'));
  // A key on one of your other Macs is written without its `@<short>:` in a conversation there.
  const cut = device.endsWith(':') ? device.length : 0;
  const whole = names.get(cut ? device + name : name);
  if (whole && mine(whole.session.key)) return whole;
  let hit: Placed | null | undefined;
  for (const at of names.values()) {
    const s = at.session;
    if (!mine(s.key)) continue;
    if (s.id === name) return at;
    if (s.id.startsWith(name) || s.key.startsWith(name, cut) || s.shortId === name) hit = hit === undefined ? at : null;
  }
  return hit;
}

/** The session `name` names, among this Mac's first and then your other Macs', as the command
 *  has it (cli.rs `session_named_anywhere`); one written with its Mac (`@<short>:…`) is that
 *  Mac's. In a conversation on another of your Macs (`away`, its short id) a key is that Mac's
 *  own, which this one lists after `@<short>:`, so only that Mac's sessions are named: one there
 *  isn't this Mac's of the same id. None when it names none, or more than one. */
export function sessionNamed(names: SessionNames, name: string, away: string | null = null): Placed | null {
  if (!name) return null;
  if (name.startsWith('@')) return namedOn(names, name, '@') ?? null;
  if (away) return namedOn(names, name, `@${away}:`) ?? null;
  const here = namedOn(names, name, '');
  return here !== undefined ? here : (namedOn(names, name, '@') ?? null);
}

/** What a link macOS opened names in a window: the session, where it lives, or the todo; or
 *  what to say when there's none. `elsewhere`: it's a session's link, and the session may be on
 *  another of your devices that the window hasn't read yet. */
export type Opened = { session: Placed } | { todo: Todo } | { none: string; elsewhere: boolean };

export function opened(url: string, names: SessionNames, todos: readonly Todo[]): Opened {
  const to = codealongLinkOf(url);
  if (to?.kind === 'session') {
    const session = sessionNamed(names, to.name);
    return session ? { session } : { none: `There’s no session ${to.name} here.`, elsewhere: true };
  }
  if (to?.kind === 'todo') {
    const todo = todoNamed(todos, to.id);
    return todo ? { todo } : { none: `There’s no todo ${to.id} here.`, elsewhere: false };
  }
  return { none: 'That link names nothing Codealong shows.', elsewhere: false };
}

/** The todo `id` names: the one with that id, else the only one whose id starts with it. */
export function todoNamed(todos: readonly Todo[], id: string): Todo | null {
  if (!id) return null;
  const exact = todos.find((t) => t.id === id);
  if (exact) return exact;
  const hits = todos.filter((t) => t.id.startsWith(id));
  return hits.length === 1 ? hits[0] : null;
}
