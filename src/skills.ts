// Skills in the `/` lists: the quick panel's, where picking a repo's also picks the repo a
// session starts in, and a session's reply box's. The engine lists them (skills.rs): each once by
// name, with what each harness that reads it writes in a prompt to run it (`/ship` for Claude
// Code, `$ship` for Codex), a repo's and an account's own (its folders', and what its harness
// has built in, as Claude Code's `/design`). Here: the `/name`s in the text and the one being
// typed at the caret, which skills it could be, what goes to the session in their place, and
// which accounts a skill leaves out. A skill is offered only on accounts whose harness reads the
// folder it's in (asked 2026-10-04), and an account's own only on the accounts that have it: the
// others would get its name as plain words. Kept free of React so the rules have tests.

import { HARNESS_WORD } from './harness';
import { newWorkIn } from './reusable';
import type { Harness, Lane, Skill, SkillsListed } from './types';

/** What a skill's name may hold, as the engine lets them through (skills.rs `typeable`). */
const NAME = String.raw`[A-Za-z0-9_:.-]`;
/** A slash and a name that start the text, and end the word: `/ship` in "/ship it" or "/ship, and",
 *  not the one in a path. Only the start runs a skill, as Claude Code takes a slash command, so a
 *  `/name` later on is words ("the /usage endpoint"; asked 2026-10-14). A path's first part is no
 *  skill, even one that names a skill (`/review/notes.md` with a skill named `review`), so the
 *  name has to end at a space, the end of the text or punctuation that ends a word in prose,
 *  never at the slash or anything else a path goes on with. A name may hold a dot (`/v1.2`), so a
 *  full stop after one is in it here; which skill it names, if any, is `namedBy`'s to say. */
const MENTION = new RegExp(String.raw`^(\s*)/(${NAME}*)(?=$|[\s,;!?)\]}"'’”])`);

/** A `/name` in the text: where its slash is, where its name ends, and the name so far. */
export interface Mention {
  name: string;
  start: number;
  end: number;
}

/** The `/name` that starts the text, as a list of it or of none. */
export function mentionsIn(text: string): Mention[] {
  const m = MENTION.exec(text);
  if (!m) return [];
  const start = m[1].length;
  return [{ name: m[2], start, end: start + 1 + m[2].length }];
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

/** A mention as it names a skill `known` says is one: by its whole name, else by the name
 *  without the full stops it ends in, which end the sentence ("then /ship.") and stay in the text
 *  after it. A name may hold a dot, so a full stop is the sentence's only when the name with it
 *  is no skill and the name without it is. Null when it names none. */
export function namedBy(m: Mention, known: (name: string) => boolean): Mention | null {
  if (known(m.name)) return m;
  const bare = m.name.replace(/\.+$/, '');
  return bare !== m.name && known(bare) ? { ...m, name: bare, end: m.start + 1 + bare.length } : null;
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
 *  it, in place of its `/name`. What names no skill the harness reads stays as typed. Of two
 *  skills of a name, the first the harness reads: a repo's that only another reads doesn't hide
 *  the account's own. */
export function writtenFor(text: string, skills: Skill[], harness: Harness): string {
  const written = (name: string) => skills.find((s) => s.name === name && takes(s, harness))?.written[harness];
  let out = '';
  let at = 0;
  for (const m of mentionsIn(text)) {
    const n = namedBy(m, (name) => !!written(name));
    const to = n && written(n.name);
    if (!n || !to) continue;
    out += text.slice(at, n.start) + to;
    at = n.end;
  }
  return out + text.slice(at);
}

/** The skill that starts the text, named whole. */
export function skillNamed(text: string, skills: Skill[]): Skill | undefined {
  for (const m of mentionsIn(text)) {
    const n = namedBy(m, (name) => !!named(skills, name));
    if (n) return named(skills, n.name);
  }
  return undefined;
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
  return `/${skill.name} isn’t where ${wordList(harnesses)} ${harnesses.length === 1 ? 'reads' : 'read'} skills, so ${wordList(out.map((a) => a.name))} ${out.length === 1 ? 'isn’t' : 'aren’t'} offered.`;
}

/** Whether a start with a quick panel row's skill is offered on `account`: its agent reads the
 *  skill, and for an account's own skill, it's one of the accounts that have it. */
export function skillOffers(row: SkillRow, account: { name: string; harness: Harness }): boolean {
  return takes(row.skill, account.harness) && (!!row.lane || row.accounts.includes(account.name));
}

/** Why some accounts aren't offered with a quick panel row's skill, in a sentence; null when
 *  all are. */
export function skillLeftOutWords(row: SkillRow, accounts: { name: string; harness: Harness }[]): string | null {
  if (row.lane) return leftOutWords(row.skill, accounts);
  const out = accounts.filter((a) => !row.accounts.includes(a.name)).map((a) => a.name);
  if (out.length === 0) return null;
  return `Only ${wordList(row.accounts)} ${row.accounts.length === 1 ? 'has' : 'have'} /${row.skill.name}, so ${wordList(out)} ${out.length === 1 ? 'isn’t' : 'aren’t'} offered.`;
}

/** Words joined as a sentence lists them: "a, b and c". */
function wordList(words: string[]): string {
  return words.length <= 1 ? words.join('') : `${words.slice(0, -1).join(', ')} and ${words[words.length - 1]}`;
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
  /** The repo it's in, by the worktree its skills were read from; null for an account's own,
   *  which picks no repo. */
  lane: Lane | null;
  /** For an account's own, the accounts that have it, by name: a start is offered only on them. */
  accounts: string[];
  /** Where the typed name is in its title (`/name`), for underlining. */
  marks: [number, number][];
}

/** Where a skill row's skill is, in a word or two: its repo, or the accounts that have it. */
export function skillWhere(row: SkillRow): string {
  return row.lane ? row.lane.repo : row.accounts.join(', ');
}

/** The quick panel's rows for a name typed so far: each repo's skills that could be it, and the
 *  accounts' own (a row for each name, whichever of them have it), the ones that start with it
 *  first, then the ones meant to be typed, then the most run. A skill in two repos is a row in
 *  each, the second after every other skill's first. With the name finished (`whole`), only the
 *  skills of exactly that name: one in several repos still has to be chosen. */
export function skillRows(listed: SkillsListed | null, repos: Lane[], word: string, whole: boolean, limit: number): SkillRow[] {
  if (!listed) return [];
  const w = word.toLowerCase();
  const found = (skills: Skill[]) => (whole ? skills.filter((s) => s.name === word) : matching(skills, word));
  const row = (skill: Skill, key: string, lane: Lane | null, accounts: string[]) => {
    const at = skill.name.toLowerCase().indexOf(w);
    const r: SkillRow = { kind: 'skill', key, group: 'skills', skill, lane, accounts, marks: w && at >= 0 ? [[at + 1, at + 1 + w.length]] : [] };
    return { row: r, rank: rankOf(skill, w) };
  };
  const ofRepos = repos.flatMap((lane) => found(listed.folders.find((f) => f.folder === lane.path)?.skills ?? []).map((skill) => row(skill, `skill:${lane.id}:${skill.name}`, lane, [])));
  // An account's own, once by name: the first account's describes it, and each says how its
  // agent is told to run it.
  const own = new Map<string, { skill: Skill; accounts: string[] }>();
  for (const a of listed.accounts) {
    for (const skill of found(a.skills)) {
      const have = own.get(skill.name);
      if (have) {
        have.accounts.push(a.account);
        have.skill = { ...have.skill, written: { ...skill.written, ...have.skill.written } };
      } else own.set(skill.name, { skill, accounts: [a.account] });
    }
  }
  const ofAccounts = [...own.values()].map(({ skill, accounts }) => row(skill, `skill:@:${skill.name}`, null, accounts));
  const rows = [...ofRepos, ...ofAccounts];
  const order = (a: (typeof rows)[number], b: (typeof rows)[number]) =>
    a.rank - b.rank ||
    Number(b.row.skill.typed) - Number(a.row.skill.typed) ||
    b.row.skill.uses - a.row.skill.uses ||
    a.row.skill.name.localeCompare(b.row.skill.name) ||
    (b.row.lane?.lastActivity ?? 0) - (a.row.lane?.lastActivity ?? 0);
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
