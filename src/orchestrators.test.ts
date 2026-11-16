import { describe, expect, it } from 'vitest';
import { orchestratorFor, orchestratorOn, orchestratorsIn, scopeOfView } from './orchestrators';
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
