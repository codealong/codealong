import { describe, expect, it } from 'vitest';
import { compassTitle, foldQuietRuns, freeScopes, orchestrates, orchestratorFor, orchestratorOn, orchestratorsIn, scopeOfView } from './orchestrators';
import { ALL_PROJECTS } from './quick';
import { lane, session } from './testing/fixtures';
import type { ScheduleItem, TranscriptItem } from './types';

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

  it('folds the runs that found nothing into a line, and leaves the ones that told you something', () => {
    let n = 0;
    const item = (kind: TranscriptItem['kind'], text = '', extra: Partial<TranscriptItem> = {}): TranscriptItem => ({ at: n * 1000, kind, text, failed: false, cursor: 1, n: n++, ...extra });
    const tick = () => item('codealong', 'You’re Codealong’s orchestrator…');
    const quiet = () => [tick(), item('tool', 'codealong changes'), item('result'), item('agent', '—')];
    const told = [tick(), item('tool', 'codealong changes'), item('agent', 'Two sessions now work in one worktree.')];
    const items = [item('you', 'Keep watch'), item('agent', 'On it.'), ...quiet(), ...quiet(), ...told, ...quiet()];
    const folded = foldQuietRuns(items);
    expect(folded.map((x) => [x.kind, x.text])).toEqual([
      ['you', 'Keep watch'],
      ['agent', 'On it.'],
      ['note', 'Looked 2 times: nothing new'],
      ...told.map((x) => [x.kind, x.text]),
      ['note', 'Looked: nothing new'],
    ]);
    // Each line keeps the place of the run it starts with, so it has a key of its own.
    expect(folded[2].n).toBe(items[2].n);
    // A run still going, its last step a call, stays.
    const going = [tick(), item('agent', '—'), item('tool', 'codealong changes')];
    expect(foldQuietRuns(going)).toEqual(going);
    // What came before the page's first message from Codealong stays too.
    const cut = [item('tool', 'codealong changes'), item('agent', '—')];
    expect(foldQuietRuns(cut)).toEqual(cut);
    // A fresh session's first run, its prompt recorded as yours, folds as the others do; a prompt
    // that is yours doesn't.
    const first = item('you', "You're Codealong's orchestrator for All projects: you keep watch.\n\nThis comes from the schedule “Orchestrator”: run for “Fix the docs” asks you a question.");
    expect(foldQuietRuns([first, item('agent', '—')]).map((x) => x.text)).toEqual(['Looked: nothing new']);
    const yours = [item('you', 'Anything new?'), item('agent', '—')];
    expect(foldQuietRuns(yours)).toEqual(yours);
    // Nor one of yours that quotes Codealong's line.
    const quoting = [item('you', 'Why did it say this?\n\nThis comes from the schedule “Orchestrator”: run for a PR.\n\nIt looked wrong.'), item('agent', '—')];
    expect(foldQuietRuns(quoting)).toEqual(quoting);
    // A tick whose words were edited in its schedule still folds.
    const edited = item('you', 'Keep an eye on the release.\n\nThis comes from the schedule “Orchestrator”: run for a PR. Its notes are in /n.md: read them first.');
    expect(foldQuietRuns([edited, item('agent', '—')]).map((x) => x.text)).toEqual(['Looked: nothing new']);
    // A run that asked you something keeps it in view, with your answers.
    const asked = [tick(), item('tool', 'AskUserQuestion', { questions: [{ question: 'Merge it?', header: 'Merge', options: [], multiSelect: false }] }), item('result', '', { answers: { 'Merge it?': ['No'] } }), item('agent', '—')];
    expect(foldQuietRuns(asked)).toEqual(asked);
  });
});
