// The quick panel's choice of how a new session starts: the account (or Auto) and model picked in
// its agent menu, the effort under them, the place it opens in and whether it gets a new
// worktree. What's offered comes from where each account can start (`options`) and their usage;
// what the picks come to is start.ts `startChoice`.

import { useMemo, useState } from 'react';
import { agentId, agentOptions, autoPick, initialChoice, parseAgent, remembered, settledPlace, startChoice, withLists } from './start';
import type { StartOptions, StartPlace, Usage } from './types';

/** What's picked, with its setters. The quick panel keeps these from one open to the next only
 *  when an open says to, so it sets them back itself. */
export function useStartPicks() {
  // The agent menu's choice; null is the first it offers, Auto where there's one.
  const [agent, setAgent] = useState<string | null>(null);
  // `--effort` for the agent chosen; null is the account's own.
  const [effort, setEffort] = useState<string | null>(null);
  const [place, setPlace] = useState<StartPlace | null>(null);
  const [newWorktree, setNewWorktree] = useState(false);
  return { agent, setAgent, effort, setEffort, place, setPlace, newWorktree, setNewWorktree };
}

/** What the picks come to among what `options` offers, and the agent menu's and places' steps. */
export function useStartChoice(picks: ReturnType<typeof useStartPicks>, options: StartOptions | null, usage: Usage[], now: number) {
  const { agent, setAgent, effort, setEffort, place, setPlace, newWorktree } = picks;

  // Stars come with the board, which is current; where accounts can start is read less often.
  const accounts = useMemo(() => (options ? withLists(options) : []).map((a) => ({ ...a, preferred: usage.find((u) => u.account === a.name)?.preferred ?? a.preferred })), [options, usage]);
  // Every account's models, for saying one by its name; each account lists its own.
  const models = useMemo(() => accounts.flatMap((a) => a.models ?? []).filter((m, i, all) => all.findIndex((x) => x.id === m.id) === i), [accounts]);
  // The effort levels the agent menu offers: those of the accounts that take one.
  const efforts = useMemo(() => accounts.find((a) => (a.efforts?.length ?? 0) > 0)?.efforts ?? [], [accounts]);
  // Auto follows usage while the panel is up, and resolves when you start.
  const pick = useMemo(() => autoPick(accounts, usage, now), [accounts, usage, now]);
  const agents = useMemo(() => agentOptions(accounts, models, pick), [accounts, models, pick]);
  const first = useMemo(() => initialChoice(accounts, pick, undefined), [accounts, pick]);
  const chosen = startChoice(accounts, models, efforts, usage, pick, agents, first, { agent, effort, place, newWorktree }, (c) => remembered().places?.[c]);
  const { choice, chosenModel } = chosen;

  /** Picks a line of the agent menu. A model is asked for on the command line; an app picks its own. */
  const pickAgent = (id: string) => {
    const { choice: c, model } = parseAgent(id);
    setAgent(id);
    if (model) setPlace('terminal');
    else setPlace(c ? (settledPlace(c, accounts, pick) ?? remembered().places?.[c] ?? null) : null);
  };

  /** Picks where it opens. The apps choose their model: an account on a model becomes the account
   *  on its default. */
  const pickPlace = (p: StartPlace) => {
    setPlace(p);
    if (p !== 'terminal' && choice && chosenModel) setAgent(agentId(choice, null));
  };

  // The effort under the agent menu's models: a setting for whichever is chosen, so it stays
  // while you look through them. It goes to Claude Code only, and not to Haiku.
  const effortSteps: (string | null)[] = [null, ...efforts.map((e) => e.id)];
  // From the effort as it is, not as last drawn: a held key can repeat before the menu redraws.
  const stepEffort = (delta: -1 | 1) =>
    setEffort((e) => effortSteps[Math.min(effortSteps.length - 1, Math.max(0, Math.max(0, effortSteps.indexOf(e)) + delta))]);

  return { ...chosen, accounts, models, efforts, pick, agents, pickAgent, pickPlace, stepEffort };
}
