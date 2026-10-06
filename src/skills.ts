// Skills in the `/` lists: the quick panel's, where picking one also picks the repo a session
// starts in, and a session's reply box's. The engine lists them (skills.rs): each once by name,
// with what each harness that reads it writes in a prompt to run it (`/ship` for Claude Code,
// `$ship` for Codex). Here: the `/name`s in the text and the one being typed at the caret, which
// skills it could be, what goes to the session in their place, and which accounts a skill leaves out. A skill is offered only on
// accounts whose harness reads the folder it's in (asked 2026-10-04): the others would get its
// name as plain words. Kept free of React so the rules have tests.

import { HARNESS_WORD } from './harness';
import { newWorkIn } from './reusable';
import type { Harness, Lane, Skill, SkillsListed } from './types';

/** What a skill's name may hold, as the engine lets them through (skills.rs `typeable`). */
const NAME = String.raw`[A-Za-z0-9_:.-]`;
/** A slash and a name, at the start of the text or after a space, that ends the word: `/ship` in
 *  "then /ship it" or "then /ship, and", not the one in a path. A path's first part is no skill,
 *  even one that names a skill (`/review/notes.md` with a skill named `review`), so the name has
 *  to end at a space, the end of the text or punctuation that ends a word in prose, never at the
 *  slash or anything else a path goes on with. */
const MENTION = new RegExp(String.raw`(^|\s)/(${NAME}*)(?=$|[\s,;!?)\]}"'’”])`, 'g');

/** A `/name` in the text: where its slash is, where its name ends, and the name so far. */
export interface Mention {
  name: string;
  start: number;
  end: number;
}

/** Every `/name` in the text, in order. */
export function mentionsIn(text: string): Mention[] {
  const out: Mention[] = [];
  for (const m of text.matchAll(MENTION)) {
    const start = m.index + m[1].length;
    out.push({ name: m[2], start, end: start + 1 + m[2].length });
  }
  return out;
}

/** The `/name` the caret is in or just after: the one being typed. */
export function mentionAt(text: string, caret: number): Mention | null {
  return mentionsIn(text).find((m) => m.start < caret && caret <= m.end) ?? null;
}

/** What's typed with a mention's name finished as `skill`'s, and a space after it unless one is
 *  there, with where the caret goes. */
export function completed(text: string, mention: Mention, skill: Pick<Skill, 'name'>): { text: string; caret: number } {
  const after = text.slice(mention.end);
  const spaced = /^\s/.test(after);
  const name = `/${skill.name}${spaced ? '' : ' '}`;
  // The caret goes past the space either way, ready for what follows.
  return { text: text.slice(0, mention.start) + name + after, caret: mention.start + name.length + (spaced ? 1 : 0) };
}

/** Where `word` is in a skill's name: 0 at its start or at the start of the part after its
 *  plugin (`figma:figma-use` for `fig`), 1 anywhere else, -1 when it isn't there. */
function rankOf(skill: Skill, word: string): number {
  const name = skill.name.toLowerCase();
  const own = name.slice(name.lastIndexOf(':') + 1);
  if (name.startsWith(word) || own.startsWith(word)) return 0;
  return name.includes(word) ? 1 : -1;
}

/** The skills a name typed so far could be: those that start with it, then those that have it,
 *  each in the engine's order (the ones meant to be typed, then the most run). */
export function matching(skills: Skill[], word: string): Skill[] {
  const w = word.toLowerCase();
  const ranked = skills.map((skill) => ({ skill, rank: rankOf(skill, w) })).filter((x) => x.rank >= 0);
  return [...ranked.filter((x) => x.rank === 0), ...ranked.filter((x) => x.rank === 1)].map((x) => x.skill);
}

/** The skill a whole name names. */
export function named(skills: Skill[], name: string): Skill | undefined {
  return skills.find((s) => s.name === name);
}

/** A session's skills from what the engine listed for its folder and account: the worktree's
 *  first, which win a name both have. Only the ones its harness can be told to run, picked
 *  before the names are made unique: a worktree's skill another harness reads doesn't hide the
 *  account's skill of the same name that this one does. */
export function sessionSkills(listed: SkillsListed | null, harness: Harness): Skill[] {
  if (!listed) return [];
  const all = [...listed.folders.flatMap((f) => f.skills), ...listed.accounts.flatMap((a) => a.skills)];
  const runnable = all.filter((s) => takes(s, harness));
  return runnable.filter((s, i) => runnable.findIndex((x) => x.name === s.name) === i);
}

/** What goes to a session on `harness` for what's typed: each skill named as the harness wants
 *  it, in place of its `/name`. What names no skill the harness reads stays as typed. */
export function writtenFor(text: string, skills: Skill[], harness: Harness): string {
  let out = '';
  let at = 0;
  for (const m of mentionsIn(text)) {
    const to = named(skills, m.name)?.written[harness];
    if (!to) continue;
    out += text.slice(at, m.start) + to;
    at = m.end;
  }
  return out + text.slice(at);
}

/** The first skill the text names, whole. */
export function skillNamed(text: string, skills: Skill[]): Skill | undefined {
  return mentionsIn(text).map((m) => named(skills, m.name)).find((s) => !!s);
}

/** Whether an account's harness can be asked to run a skill. */
export function takes(skill: Skill, harness: Harness): boolean {
  return !!skill.written[harness];
}

/** Why some accounts aren't offered for a skill, in a sentence; null when all are. */
export function leftOutWords(skill: Skill, accounts: { name: string; harness: Harness }[]): string | null {
  const out = accounts.filter((a) => !takes(skill, a.harness));
  if (out.length === 0) return null;
  const harnesses = [...new Set(out.map((a) => HARNESS_WORD[a.harness]))];
  const list = (words: string[]) => (words.length <= 1 ? words.join('') : `${words.slice(0, -1).join(', ')} and ${words[words.length - 1]}`);
  return `/${skill.name} isn’t where ${list(harnesses)} ${harnesses.length === 1 ? 'reads' : 'read'} skills, so ${list(out.map((a) => a.name))} ${out.length === 1 ? 'isn’t' : 'aren’t'} offered.`;
}

/* ---- The quick panel: a skill picks its repo ---- */

/** One worktree a repo on this Mac, to read the repo's skills from: its main checkout, else
 *  the first of its worktrees listed. Chats and other devices' worktrees have none to read. */
export function repoLanes(lanes: Lane[]): Lane[] {
  const out = new Map<string, Lane>();
  for (const l of lanes) {
    if (l.device || !l.repo) continue;
    const key = `${l.project}\n${l.repo}`;
    const have = out.get(key);
    if (!have || (l.origin === 'main' && have.origin !== 'main')) out.set(key, l);
  }
  return [...out.values()];
}

export interface SkillRow {
  kind: 'skill';
  key: string;
  group: 'skills';
  skill: Skill;
  /** The repo it's in, by the worktree its skills were read from. */
  lane: Lane;
  /** Where the typed name is in its title (`/name`), for underlining. */
  marks: [number, number][];
}

/** The quick panel's rows for a name typed so far: each repo's skills that could be it, the ones
 *  that start with it first, then the ones meant to be typed, then the most run. A skill in two
 *  repos is a row in each, the second after every other skill's first. With the name finished (`whole`), only the skills of exactly that
 *  name: one in several repos still has to be chosen. */
export function skillRows(listed: SkillsListed | null, repos: Lane[], word: string, whole: boolean, limit: number): SkillRow[] {
  if (!listed) return [];
  const w = word.toLowerCase();
  const rows = repos.flatMap((lane) => {
    const skills = listed.folders.find((f) => f.folder === lane.path)?.skills ?? [];
    const found = whole ? skills.filter((s) => s.name === word) : matching(skills, word);
    return found.map((skill) => {
      const at = skill.name.toLowerCase().indexOf(w);
      const row: SkillRow = { kind: 'skill', key: `skill:${lane.id}:${skill.name}`, group: 'skills', skill, lane, marks: w && at >= 0 ? [[at + 1, at + 1 + w.length]] : [] };
      return { row, rank: rankOf(skill, w) };
    });
  });
  const order = (a: (typeof rows)[number], b: (typeof rows)[number]) =>
    a.rank - b.rank || Number(b.row.skill.typed) - Number(a.row.skill.typed) || b.row.skill.uses - a.row.skill.uses || a.row.skill.name.localeCompare(b.row.skill.name) || b.row.lane.lastActivity - a.row.lane.lastActivity;
  const sorted = rows.sort(order).map((x) => x.row);
  // Each name's first row before any name's second, so a skill many repos have doesn't fill
  // the list: typing more of its name leaves only its rows.
  const firsts = sorted.filter((r, i) => sorted.findIndex((x) => x.skill.name === r.skill.name) === i);
  return [...firsts, ...sorted.filter((r) => !firsts.includes(r))].slice(0, limit);
}

/** Where a session started on a repo's skill goes, as new work goes anywhere (reusable.ts
 *  `newWorkIn`): a reusable worktree of the repo that holds no work, else its main worktree
 *  when that's reusable; with no reusable worktree at all, a new one off the main checkout. */
export function skillStart(repo: Lane, lanes: Lane[]): { lane: Lane; newWorktree: boolean } {
  const reusable = newWorkIn(repo, 0, lanes, []);
  return { lane: reusable ?? repo, newWorktree: !reusable };
}
