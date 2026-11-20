// The decision models as Settings shows them (decisions.rs): the providers Codealong has an
// adapter for, the models config.toml names with Jev always among them, and which model reads a
// project's or a boundary group's sessions. Pure functions, so tests can pin them to the engine's
// rules (`models`, `for_project`, `for_session`).

import type { Config, DecisionModelCfg, DecisionsCfg, ProjectCfg, RoutingGroupCfg } from './types';

/** A provider Codealong has an adapter for (decisions.rs `PROVIDERS`), with what Settings says of it. */
export interface Provider {
  id: string;
  /** Its name in words (`Adapter::word`). */
  word: string;
  /** The model a new entry for it starts on (`Adapter::default_model`). */
  model: string;
  /** A name a new entry for it starts with. */
  name: string;
  /** Where to make a key. */
  keys: string;
  /** What it charges, in words. */
  price: string;
}

export const PROVIDERS: Provider[] = [
  { id: 'typesafe', word: 'TypeSafe', model: 'jev-preview', name: 'jev', keys: 'https://console.typesafe.ai/keys', price: '$0.042 per million tokens sent' },
  { id: 'openai', word: 'OpenAI', model: 'gpt-6-luna', name: 'luna', keys: 'https://platform.openai.com/api-keys', price: '$0.10 per million tokens sent, $0.01 for ones it has cached' },
  { id: 'gemini', word: 'Gemini', model: 'gemini-decision-1', name: 'gemini', keys: 'https://aistudio.google.com/apikey', price: '$0.10 per million tokens sent on its paid tier' },
];

export function provider(id: string): Provider | undefined {
  return PROVIDERS.find((p) => p.id === id.trim());
}

/** `[decisions]` when config.toml has none (config.rs `DecisionsCfg`): Jev the default, every
 *  feature off. */
export const DECISIONS_OFF: DecisionsCfg = { default: 'jev', quick_panel: false, sessions: false, todos: false, pull_requests: false, prompts: false, models: [] };

/** Jev as config.toml has it without a `[[decisions.models]]` entry of its own. */
export const JEV: DecisionModelCfg = { name: 'jev', provider: 'typesafe', model: 'jev-preview' };

export function decisionsOf(cfg: Config): DecisionsCfg {
  return { ...DECISIONS_OFF, ...cfg.decisions };
}

/** Every model config.toml names, Jev first when it has no entry of its own (decisions.rs `models`). */
export function modelsOf(d: DecisionsCfg): DecisionModelCfg[] {
  const models = d.models ?? [];
  return models.some((m) => m.name.trim() === 'jev') ? models : [JEV, ...models];
}

/** The models as `[[decisions.models]]` keeps them: Jev's entry left out while it's Jev as
 *  config.toml has it without one, so a change to another model doesn't write it. */
export function withModels(models: DecisionModelCfg[]): DecisionModelCfg[] {
  return models.filter((m) => !(m.name === JEV.name && m.provider === JEV.provider && m.model === JEV.model && !m.fallback));
}

/** Why a model can't be added as it's written, in words; null when it can. */
export function newModelProblem(d: DecisionsCfg, m: DecisionModelCfg): string | null {
  const name = m.name.trim();
  if (!name) return 'Give it a name.';
  if (name === 'none') return '“none” is what a project says for no model.';
  if (modelsOf(d).some((x) => x.name.trim() === name)) return `There’s a model named ${name} already.`;
  if (!m.model.trim()) return 'Name the provider’s model.';
  return null;
}

/** `d` without the model named `name`, and nothing left naming it: a fallback, the default, a
 *  project's or a boundary group's choice of it goes back to the default's (Jev for a group, as
 *  boundary.rs has one that names none). */
export function withoutModel(cfg: Config, name: string): Config {
  const d = decisionsOf(cfg);
  const models = (d.models ?? []).filter((m) => m.name !== name).map((m) => (m.fallback === name ? { ...m, fallback: undefined } : m));
  return {
    ...cfg,
    decisions: { ...d, models, default: d.default === name ? 'jev' : d.default },
    projects: cfg.projects.map((p) => (p.decisions === name ? { ...p, decisions: undefined } : p)),
    routing: cfg.routing && { ...cfg.routing, groups: (cfg.routing.groups ?? []).map((g) => (g.decisions === name ? { ...g, decisions: undefined } : g)) },
  };
}

/** The model that reads what's said in `project`: its `decisions`, else the default; null for
 *  none (decisions.rs `for_project`). */
export function forProject(d: DecisionsCfg, project: ProjectCfg | undefined): string | null {
  const said = project?.decisions?.trim() || d.default.trim() || 'jev';
  return said === 'none' ? null : said;
}

/** The model a boundary group lets read its sessions: its `decisions`, Jev when it names none,
 *  whatever the default is (boundary.rs `decisions`). */
export function forGroup(g: RoutingGroupCfg): string | null {
  const said = g.decisions?.trim() || 'jev';
  return said === 'none' ? null : said;
}

/** A model's choices for a project's or group's select: the default (for a project), each model,
 *  and none. */
export function choices(d: DecisionsCfg, withDefault: boolean): { id: string; name: string }[] {
  const def = d.default.trim() === 'none' ? 'none' : d.default.trim() || 'jev';
  return [
    ...(withDefault ? [{ id: '', name: `The default (${def})` }] : []),
    ...modelsOf(d).map((m) => ({ id: m.name, name: `${m.name}: ${provider(m.provider)?.word ?? m.provider} ${m.model}` })),
    { id: 'none', name: 'None: what’s said goes to no decision model' },
  ];
}

/** A model as the usage report keys it (`<provider>:<model>`, ledger.rs `JevDay::models`), in words. */
export function usedModelName(key: string): string {
  const at = key.indexOf(':');
  if (at < 0) return key;
  const p = provider(key.slice(0, at));
  return `${p?.word ?? key.slice(0, at)} ${key.slice(at + 1)}`;
}

/** The providers whose keys Settings asks for: each one a model uses, TypeSafe's always. */
export function providersInUse(d: DecisionsCfg): Provider[] {
  const used = new Set(['typesafe', ...modelsOf(d).map((m) => m.provider.trim())]);
  return PROVIDERS.filter((p) => used.has(p.id));
}
