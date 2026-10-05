// Skills in the `/` lists: the quick panel's, where picking one also picks the repo a session
// starts in, and a session's reply box's. The engine lists them (skills.rs): each once by name,
// with what each harness that reads it writes in a prompt to run it (`/ship` for Claude Code,
// `$ship` for Codex). Here: what's typed so far, which skills it could be, what goes to the
// session in its place, and which accounts a skill leaves out. A skill is offered only on
// accounts whose harness reads the folder it's in (asked 2026-10-04): the others would get its
// name as plain words. Kept free of React so the rules have tests.

import { HARNESS_WORD } from './harness';
import { newWorkIn } from './reusable';
import type { Harness, Lane, Skill, SkillsListed } from './types';

/** What a skill's name may hold, as the engine lets them through (skills.rs `typeable`). */
const NAME = String.raw`[A-Za-z0-9_:.-]`;
const SLASH = new RegExp(String.raw`^/(${NAME}*)(\s[\s\S]*)?$`);

/** What's typed, when it starts with a slash and a name: the name so far, what follows it, and
 *  whether the name is still being typed (nothing after it yet, not a space). */
export function slashAt(text: string): { name: string; rest: string; open: boolean } | null {
  const m = SLASH.exec(text.trimStart());
  return m ? { name: m[1], rest: (m[2] ?? '').trim(), open: m[2] === undefined } : null;
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
 *  first, which win a name both have. Only the ones its harness can be told to run. */
export function sessionSkills(listed: SkillsListed | null, harness: Harness): Skill[] {
  if (!listed) return [];
  const all = [...listed.folders.flatMap((f) => f.skills), ...listed.accounts.flatMap((a) => a.skills)];
  return all.filter((s, i) => !!s.written[harness] && all.findIndex((x) => x.name === s.name) === i);
}

/** What goes to a session on `harness` for what's typed: the skill's name as its harness wants
 *  it, in place of `/name`. As typed when it doesn't start with a skill the harness reads. */
export function writtenFor(text: string, skills: Skill[], harness: Harness): string {
  const at = slashAt(text);
  const to = at && named(skills, at.name)?.written[harness];
  if (!at || !to) return text;
  const start = text.length - text.trimStart().length;
  return text.slice(0, start) + to + text.slice(start + 1 + at.name.length);
}

/** What's typed with the name finished as `skill`'s, ready for what follows it. */
export function completed(text: string, skill: Pick<Skill, 'name'>): string {
  const at = slashAt(text);
  return `/${skill.name} ${at?.rest ?? ''}`;
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
