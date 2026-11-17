import { describe, expect, it } from 'vitest';
import { compassTitle, freeScopes, orchestrates, orchestratorFor, orchestratorOn, orchestratorsIn, scopeOfView } from './orchestrators';
import { ALL_PROJECTS } from './quick';
import { lane, session } from './testing/fixtures';
import type { ScheduleItem } from './types';

const chats = lane({ id: '/chats', project: 'Elsewhere' });
const row = (key: string, project?: string, archived = false) => ({ lane: chats, session: session({ key, archived, orchestrator: { schedule: key, project } }) });

describe('orchestrators', () => {
  it('leads a project with its own, and All projects with every one, All projects’ first', () => {
    const rows = [row('web', 'Web'), row('all'), row('docs', 'Docs'), { lane: chats, session: session({ key: 'plain' }) }];
    expect(orchestratorsIn(rows, 'web').map((r) => r.session.key)).toEqual(['web']);
    expect(orchestratorsIn(rows, 'Atlas')).toEqual([]);
    expect(orchestratorsIn(rows, ALL_PROJECTS).map((r) => r.session.key)).toEqual(['all', 'docs', 'web']);
    // One that's archived (turned off, or started afresh) leads nothing.
    expect(orchestratorsIn([row('web', 'Web', true)], 'Web')).toEqual([]);
  });

  it('is offered for All projects and your projects, not Elsewhere', () => {
    const projects = [{ name: 'Atlas' }, { name: 'Elsewhere' }];
    expect(orchestrates(ALL_PROJECTS, projects)).toBe(true);
    expect(orchestrates('Atlas', projects)).toBe(true);
    expect(orchestrates('Elsewhere', projects)).toBe(false);
  });

  it('offers under All projects every scope with none running, and under a project that one', () => {
    const projects = [{ name: 'Atlas' }, { name: 'Kestrel' }, { name: 'Elsewhere' }];
    const item = (id: string, project?: string, paused = false) => ({ id, paused, orchestrates: { project } }) as unknown as ScheduleItem;
    expect(freeScopes(ALL_PROJECTS, projects, [])).toEqual([null, 'Atlas', 'Kestrel']);
    // All projects’ running, and Atlas’s: Kestrel is still free, from All projects too.
    const running = [item('all'), item('atlas', 'Atlas')];
    expect(freeScopes(ALL_PROJECTS, projects, running)).toEqual(['Kestrel']);
    expect(freeScopes('Atlas', projects, running)).toEqual([]);
    expect(freeScopes('Kestrel', projects, running)).toEqual(['Kestrel']);
    // One turned off is free again.
    expect(freeScopes('Atlas', projects, [item('atlas', 'Atlas', true)])).toEqual(['Atlas']);
    expect(freeScopes('Elsewhere', projects, [])).toEqual([]);
    expect(compassTitle(['Kestrel'])).toMatch(/^Start an orchestrator for Kestrel:/);
    expect(compassTitle([null])).toMatch(/^Start an orchestrator for All projects:/);
    expect(compassTitle([null, 'Kestrel'])).toMatch(/^Start an orchestrator for All projects or one of your projects:/);
    // All projects' running already, it isn't offered.
    expect(compassTitle(['Atlas', 'Kestrel'])).toMatch(/^Start an orchestrator for one of your projects:/);
  });

  it('finds a scope’s orchestrator and whether it runs', () => {
    const item = (id: string, project?: string, paused = false) => ({ id, paused, orchestrates: { project } }) as unknown as ScheduleItem;
    const items = [item('web', 'Web', true), item('all')];
    expect(orchestratorFor(items, scopeOfView('web'))?.id).toBe('web');
    expect(orchestratorFor(items, scopeOfView(ALL_PROJECTS))?.id).toBe('all');
    expect(orchestratorFor(items, 'Docs')).toBeUndefined();
    expect(orchestratorOn(orchestratorFor(items, 'Web'))).toBe(false);
    expect(orchestratorOn(orchestratorFor(items, null))).toBe(true);
  });
});
