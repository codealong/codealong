import { describe, expect, it } from 'vitest';
import { choices, DECISIONS_OFF, forGroup, forProject, modelsOf, newModelProblem, providersInUse, usedModelName, withModels, withoutModel } from './decisions';
import type { Config, DecisionsCfg } from './types';

const luna = { name: 'luna', provider: 'openai', model: 'gpt-6-luna', fallback: 'jev' };
const d: DecisionsCfg = { ...DECISIONS_OFF, models: [luna] };

describe('decision models', () => {
  it('have Jev among them without an entry, and its own entry when it has one', () => {
    expect(modelsOf(DECISIONS_OFF).map((m) => m.name)).toEqual(['jev']);
    expect(modelsOf(d).map((m) => m.name)).toEqual(['jev', 'luna']);
    const pinned = { name: 'jev', provider: 'typesafe', model: 'jev-1.13.0' };
    expect(modelsOf({ ...d, models: [luna, pinned] })).toEqual([luna, pinned]);
  });

  it('read a project by its choice, else the default, and a boundary group by its own, Jev when it names none', () => {
    expect(forProject(d, { name: 'atlas', repos: [], folders: [], decisions: 'luna' })).toBe('luna');
    expect(forProject(d, { name: 'kestrel', repos: [], folders: [], decisions: 'none' })).toBe(null);
    expect(forProject({ ...d, default: 'luna' }, { name: 'northwind', repos: [], folders: [] })).toBe('luna');
    expect(forProject({ ...d, default: 'none' }, undefined)).toBe(null);
    expect(forGroup({ name: 'work', strategy: 'usage', members: [], boundary: true })).toBe('jev');
    expect(forGroup({ name: 'work', strategy: 'usage', members: [], boundary: true, decisions: 'none' })).toBe(null);
  });

  it('keep Jev out of the file until something about it changes', () => {
    expect(withModels(modelsOf(d))).toEqual([luna]);
    const falling = modelsOf(d).map((m) => (m.name === 'jev' ? { ...m, fallback: 'luna' } : m));
    expect(withModels(falling).map((m) => m.name)).toEqual(['jev', 'luna']);
  });

  it('turn down a model that clashes with one there is', () => {
    expect(newModelProblem(d, { name: 'sol', provider: 'gemini', model: 'gemini-decision-1' })).toBe(null);
    expect(newModelProblem(d, { name: 'luna', provider: 'gemini', model: 'x' })).toMatch(/already/);
    expect(newModelProblem(d, { name: 'jev', provider: 'gemini', model: 'x' })).toMatch(/already/);
    expect(newModelProblem(d, { name: 'none', provider: 'gemini', model: 'x' })).toMatch(/none/);
    expect(newModelProblem(d, { name: 'sol', provider: 'gemini', model: ' ' })).toMatch(/model/);
  });

  it('take a removed model out of everything that named it', () => {
    const cfg = {
      decisions: { ...d, default: 'luna', models: [luna, { name: 'sol', provider: 'gemini', model: 'g', fallback: 'luna' }] },
      projects: [{ name: 'atlas', repos: [], folders: [], decisions: 'luna' }],
      routing: { groups: [{ name: 'work', strategy: 'usage', members: [], boundary: true, decisions: 'luna' }] },
    } as unknown as Config;
    const out = withoutModel(cfg, 'luna');
    expect(out.decisions?.models?.map((m) => [m.name, m.fallback])).toEqual([['sol', undefined]]);
    expect(out.decisions?.default).toBe('jev');
    expect(out.projects[0].decisions).toBe(undefined);
    expect(out.routing?.groups?.[0].decisions).toBe(undefined);
  });

  it('are offered and named as the windows say them', () => {
    expect(choices(d, true).map((c) => c.id)).toEqual(['', 'jev', 'luna', 'none']);
    expect(choices(d, false)[0].name).toBe('jev: TypeSafe jev-preview');
    expect(usedModelName('openai:gpt-6-luna')).toBe('OpenAI gpt-6-luna');
    expect(usedModelName('typesafe:jev-1.14.0')).toBe('TypeSafe jev-1.14.0');
    expect(providersInUse(d).map((p) => p.id)).toEqual(['typesafe', 'openai']);
  });
});
