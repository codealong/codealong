// @vitest-environment jsdom
import { createRoot } from 'react-dom/client';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';
import { act } from '../testing/dom';
import * as fx from '../testing/fixtures';
import type { HelperCount, RunCount, SideCount } from '../boardLists';
import { ALL_PROJECTS } from '../quick';
import { blankSchedule } from '../schedules';
import type { ScheduleItem } from '../types';
import { GroupBlock, type GroupActs } from './Groups';
import { AgentRow } from './LaneRow';
import { PinnedSection } from './PinnedSection';
import { ProjectIcons, projectIconMap, ProjectShown } from './ProjectSaid';
import { RepoChip, WorktreeChip } from './primitives';
import { PrsView, type PrRowItem } from './PrsView';
import { SchedulesView } from './SchedulesView';
import { TodoView } from './TodoView';

const ICON_A = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==';
const ICON_N = 'data:image/png;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';

const ICONS = projectIconMap([
  { name: 'Atlas', icon: ICON_A },
  { name: 'Northwind', icon: ICON_N },
  { name: 'Personal' },
]);

function mount(node: ReactNode, project = ALL_PROJECTS) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() =>
    root.render(
      <ProjectShown.Provider value={project}>
        <ProjectIcons.Provider value={ICONS}>{node}</ProjectIcons.Provider>
      </ProjectShown.Provider>,
    ),
  );
  return {
    el,
    unmount: () => {
      act(() => root.unmount());
      el.remove();
    },
  };
}

const glance = (status: 'needs' | 'working' | 'idle', key = 'h1') => ({
  key,
  title: 'Helper',
  status,
  since: fx.NOW - 60_000,
  review: null,
  excerpt: null,
  account: 'claude',
  model: null,
  worktree: null,
  reports: false,
});

describe('WorktreeChip and RepoChip org icon & worktree naming', () => {
  it('replaces the folder icon with the project org avatar and hides disposable worktree slugs', () => {
    const disposable = fx.lane({ project: 'Northwind', repo: 'northwind', callsign: 'shimmering-greeting-dusk', origin: 'claude-app', reuse: undefined });
    const main = fx.lane({ project: 'Atlas', repo: 'atlas', callsign: 'atlas', origin: 'main', reuse: 'open' });
    const reusable = fx.lane({ project: 'Northwind', repo: 'northwind', callsign: 'kestrel', origin: 'script', reuse: 'open' });
    const noIcon = fx.lane({ project: 'Personal', repo: 'notes', callsign: 'draft-slug', origin: 'script', reuse: undefined });

    const view = mount(
      <>
        <WorktreeChip lane={disposable} short />
        <WorktreeChip lane={main} short />
        <WorktreeChip lane={main} />
        <WorktreeChip lane={reusable} short />
        <WorktreeChip lane={noIcon} short />
        <RepoChip repo="atlas" slug="atlas/atlas" project="Atlas" />
        <RepoChip repo="sdk" slug="northwind/sdk" project="Northwind" />
      </>,
    );
    try {
      const chips = [...view.el.querySelectorAll('.ca-wtchip, .ca-repochip')];
      // 0: Northwind disposable -> org avatar, NO folder svg, NO slug.
      expect(chips[0].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_N);
      expect(chips[0].querySelector('svg.ca-icon')).toBeNull();
      expect(chips[0].textContent).toBe('Northwind');

      // 1: Atlas main on 1 line -> org avatar (no missing avatar bug!), NO folder svg, NO / main.
      expect(chips[1].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_A);
      expect(chips[1].querySelector('svg.ca-icon')).toBeNull();
      expect(chips[1].textContent).toBe('Atlas');

      // 2: Atlas main on 2 lines -> org avatar + Atlas / main.
      expect(chips[2].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_A);
      expect(chips[2].textContent).toBe('Atlas/ main');

      // 3: Northwind reusable slot -> org avatar + Northwind / kestrel, NO folder svg.
      expect(chips[3].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_N);
      expect(chips[3].querySelector('svg.ca-icon')).toBeNull();
      expect(chips[3].textContent).toBe('Northwindkestrel');

      // 4: Personal (no org icon) -> falls back to folder svg, hides disposable slug.
      expect(chips[4].querySelector('img.ca-projicon')).toBeNull();
      expect(chips[4].querySelector('svg.ca-icon')).not.toBeNull();
      expect(chips[4].textContent).toBe('Personalnotes');

      // 5 & 6: RepoChip replaces book icon with org avatar and uses project casing when matching.
      expect(chips[5].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_A);
      expect(chips[5].querySelector('svg.ca-icon')).toBeNull();
      expect(chips[5].textContent).toBe('Atlas');
      expect(chips[6].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_N);
      expect(chips[6].querySelector('svg.ca-icon')).toBeNull();
      expect(chips[6].textContent).toBe('Northwindsdk');
    } finally {
      view.unmount();
    }
  });
});

describe('AgentRow signal cleanup', () => {
  it('hides finished helper/side/run chips, strips duplicate working duration, and suppresses Quiet on orchestrators', () => {
    const lane = fx.lane({ project: 'Atlas', repo: 'atlas', callsign: 'enchanted-slug', origin: 'script', reuse: undefined });
    const since = fx.NOW - 16 * 60 * 60_000 - 58 * 60_000;
    const working = fx.session({
      key: 'c:work',
      title: 'Mobile parity',
      status: 'working',
      statusSince: since,
      lastPromptAt: since,
      lastActivity: fx.NOW - 2_000,
      detail: `{for:${since}} · running \`git status\``,
      insight: { ending: 'offer' },
    });
    const orch = fx.session({
      key: 'c:orch',
      title: 'Orchestrator',
      status: 'idle',
      quiet: true,
      orchestrator: { schedule: 's-1', project: 'Atlas' },
    });
    const idleHelpers: HelperCount = { count: 2, working: 0, waiting: 0, each: [glance('idle', 'h1'), glance('idle', 'h2')] };
    const activeHelpers: HelperCount = { count: 2, working: 0, waiting: 1, each: [glance('needs', 'h1'), glance('idle', 'h2')] };
    const answeredSides: SideCount = { count: 4, working: 0, waiting: 0, each: [glance('idle', 's1')] };
    const doneRuns: RunCount = { count: 3, working: 0, waiting: 0, each: [glance('idle', 'r1')] };

    const view = mount(
      <>
        <AgentRow
          lane={lane}
          session={working}
          domId="row-work-idle-kids"
          now={fx.NOW}
          slim
          selected={false}
          onSelect={() => {}}
          onOpen={() => {}}
          helpers={idleHelpers}
          sides={answeredSides}
          runs={doneRuns}
        />
        <AgentRow
          lane={lane}
          session={working}
          domId="row-work-active-kids"
          now={fx.NOW}
          slim
          selected={false}
          onSelect={() => {}}
          onOpen={() => {}}
          helpers={activeHelpers}
        />
        <AgentRow
          lane={lane}
          session={orch}
          domId="row-orch"
          now={fx.NOW}
          slim
          selected={false}
          onSelect={() => {}}
          onOpen={() => {}}
        />
      </>,
    );
    try {
      const r1 = view.el.querySelector('#row-work-idle-kids')!;
      expect(r1.querySelector('.ca-helperschip')).toBeNull();
      expect(r1.querySelector('.ca-sideschip')).toBeNull();
      expect(r1.querySelector('.ca-runschip')).toBeNull();
      // Working duration moves to the age column and is stripped from detail.
      expect(r1.querySelector('.ca-seg-age')?.textContent).toBe('16h');
      expect(r1.querySelector('.ca-agentline-detail')?.textContent).toBe('running git status');
      // Disposable worktree slug is omitted.
      expect(r1.querySelector('.ca-wtchip')?.textContent).toBe('Atlas');

      const r2 = view.el.querySelector('#row-work-active-kids')!;
      expect(r2.querySelector('.ca-helperschip')).not.toBeNull();

      const r3 = view.el.querySelector('#row-orch')!;
      expect(r3.querySelector('.ca-quietchip')).toBeNull();
    } finally {
      view.unmount();
    }
  });
});

describe('PinnedSection reorder hint', () => {
  it('omits ReorderHint for a single pinned item and marks it hover-only for 2+ items', () => {
    const view = mount(
      <>
        <div id="one">
          <PinnedSection items={['a']} row={(x) => <div key={x}>{x}</div>} />
        </div>
        <div id="two">
          <PinnedSection items={['a', 'b']} row={(x) => <div key={x}>{x}</div>} />
        </div>
      </>,
    );
    try {
      expect(view.el.querySelector('#one .ca-keyhint')).toBeNull();
      const hint = view.el.querySelector('#two .ca-keyhint');
      expect(hint).not.toBeNull();
      expect(hint?.classList.contains('is-hover-hint')).toBe(true);
    } finally {
      view.unmount();
    }
  });
});

describe('PRs, Todo, and Schedules row cleanups', () => {
  it('hides diff on one-line PR rows and disposable worktrees on two-line PR rows', () => {
    const disposable = fx.lane({ project: 'Northwind', repo: 'sdk', callsign: 'random-slug', origin: 'script', reuse: undefined });
    const reusable = fx.lane({ project: 'Northwind', repo: 'sdk', callsign: 'kestrel', origin: 'script', reuse: 'review' });
    const row = (n: number, lane: ReturnType<typeof fx.lane>): PrRowItem => ({
      kind: 'pr',
      key: `northwind/sdk#${n}`,
      pr: fx.prItem({ number: n, repo: 'sdk', repoSlug: 'northwind/sdk', project: 'Northwind', additions: 40, deletions: 10 }),
      lane,
      sessions: [],
      places: [],
      primary: null,
    });
    const common = {
      pinned: [],
      sections: [{ id: 'mine' as const, label: 'Yours', rows: [row(1, disposable), row(2, reusable)] }],
      sort: 'status' as const,
      now: fx.NOW,
      selectedKey: null,
      picked: new Set<string>(),
      domId: (r: PrRowItem) => `row-${r.key}`,
      showDone: false,
      onToggleDone: () => {},
      onSelect: () => {},
      onOpen: () => {},
      onOpenSession: () => {},
      onContextMenu: () => {},
      onPlacePinned: () => {},
      onUnpin: () => {},
    };
    const view = mount(
      <>
        <div id="pr-one">
          <PrsView {...common} lines="one" />
        </div>
        <div id="pr-two">
          <PrsView {...common} lines="two" />
        </div>
      </>,
    );
    try {
      expect(view.el.querySelector('#pr-one .ca-diff')).toBeNull();
      expect(view.el.querySelector('#pr-two .ca-diff')).not.toBeNull();
      // Disposable worktree omitted on 2-line PR row; reusable slot kept.
      expect(view.el.querySelector('#pr-two [id="row-northwind/sdk#1"] .ca-wtchip')).toBeNull();
      expect(view.el.querySelector('#pr-two [id="row-northwind/sdk#2"] .ca-wtchip')?.textContent).toBe('kestrel');
    } finally {
      view.unmount();
    }
  });

  it('hides attachment counts on one-line Todo rows and renders schedule descriptions as quiet text', () => {
    const todo = fx.todo({ id: 't1', project: 'Atlas', title: 'Ship it', description: 'See [image](img.png) for details' });
    const todoView = mount(
      <TodoView
        sections={[{ id: 'ready', label: 'Ready', rows: [{ kind: 'todo', key: 't1', todo, labels: [], lane: null, primary: null, sessions: [], places: [], lastActivity: fx.NOW }] }]}
        todos={[todo]}
        sort="status"
        lines="one"
        now={fx.NOW}
        selectedKey={null}
        picked={new Set()}
        domId={(r) => `row-${r.key}`}
        canReorder
        onSelect={() => {}}
        onOpen={() => {}}
        onContextMenu={() => {}}
        onOpenSession={() => {}}
        onShowPr={() => {}}
        onOpenPr={() => {}}
        onFilterLabel={() => {}}
        onMove={() => {}}
        showDone={false}
        onToggleDone={() => {}}
        showArchived={false}
        onToggleArchived={() => {}}
        composer={null}
        empty={null}
      />,
    );
    const sched: ScheduleItem = { ...blankSchedule('/Users/me/projects/atlas'), id: 's1', name: 'Morning triage', whenWords: 'Weekdays at 9:00', runs: [], asked: 0 };
    const schedView = mount(
      <SchedulesView
        listing={{ sections: [{ id: 'active', rows: [sched] }, { id: 'oneOff', rows: [] }, { id: 'finished', rows: [] }], grouped: null }}
        groupActs={{ onSelect: () => {}, onOpen: () => {}, onOpenTarget: () => {}, onMenu: () => {}, onPrMenu: () => {}, onPrShow: () => {}, onPrOpen: () => {}, onToggle: () => {} }}
        sort="next"
        slim
        harnessItems={[]}
        now={fx.NOW}
        selectedId={null}
        selectedSession={null}
        sessionTitle={() => null}
        sessionsOf={() => ({ rows: [], more: 0 })}
        placed={() => null}
        allShown={new Set()}
        showFinished={false}
        onToggleFinished={() => {}}
        onToggleAll={() => {}}
        onSelect={() => {}}
        onEdit={() => {}}
        onMenu={() => {}}
        onSelectSession={() => {}}
        onOpenSession={() => {}}
        onSessionMenu={() => {}}
        onOpenHarness={() => {}}
        onNew={() => {}}
      />,
    );
    try {
      expect(todoView.el.querySelector('.ca-todorow-attached')).toBeNull();
      const does = schedView.el.querySelector('.ca-schedrow-does');
      expect(does).not.toBeNull();
      expect(does?.classList.contains('ca-chip')).toBe(false);
    } finally {
      todoView.unmount();
      schedView.unmount();
    }
  });

  it('collapses groups via chevron without selecting worktree headings, indents children in .ca-group-body, and hides repeated project under ProjectHead', () => {
    const lane = fx.lane({ id: '/r/web-wt', project: 'Northwind', repo: 'web', callsign: 'dusk' });
    const session = fx.session({ key: 'claude:a:g1', title: 'Grouped work' });
    const actions: string[] = [];
    const baseActs: GroupActs = {
      onSelect: () => void actions.push('select'),
      onOpen: () => {},
      onOpenTarget: () => {},
      onMenu: () => {},
      onPrMenu: () => {},
      onPrShow: () => {},
      onPrOpen: () => {},
      onToggle: () => {},
      onCollapse: (g) => void actions.push(`collapse:${g.fold}`),
    };
    const wtGroup = { key: lane.id, by: 'worktree' as const, lane, repo: null, project: null, rows: [session], all: [session], folded: 0, fold: `worktree/${lane.id}`, opened: false };
    const projGroup = { key: 'Northwind', by: 'project' as const, lane: null, repo: null, project: 'Northwind', rows: [session], all: [session], folded: 0, fold: 'project/Northwind', opened: false };
    const openView = mount(
      <>
        <div id="wt-open">
          <GroupBlock g={wtGroup} acts={{ ...baseActs, collapsed: () => false }} now={fx.NOW} rows none="No worktree">
            <AgentRow lane={lane} session={session} domId="row-g1" now={fx.NOW} slim selected={false} onSelect={() => {}} onOpen={() => {}} inWorktree />
          </GroupBlock>
        </div>
        <div id="wt-closed">
          <GroupBlock g={wtGroup} acts={{ ...baseActs, collapsed: () => true }} now={fx.NOW} rows none="No worktree">
            <AgentRow lane={lane} session={session} domId="row-g2" now={fx.NOW} slim selected={false} onSelect={() => {}} onOpen={() => {}} inWorktree />
          </GroupBlock>
        </div>
        <div id="proj-open">
          <GroupBlock g={projGroup} acts={{ ...baseActs, collapsed: () => false }} now={fx.NOW} rows={false} none="No project">
            <AgentRow lane={lane} session={session} domId="row-g3" now={fx.NOW} slim selected={false} onSelect={() => {}} onOpen={() => {}} />
          </GroupBlock>
        </div>
      </>,
    );
    try {
      // Open group renders indented .ca-group-body with its child; collapsed group hides it.
      expect(openView.el.querySelector('#wt-open .ca-group-body .ca-lane.is-agent')).not.toBeNull();
      expect(openView.el.querySelector('#wt-closed .ca-group-body')).toBeNull();
      expect(openView.el.querySelector('#wt-open .ca-grouphead-count')?.textContent).toBe('1');
      // Clicking the worktree heading's chevron collapses without firing onSelect.
      const toggle = openView.el.querySelector<HTMLButtonElement>('#wt-open .ca-grouphead-toggle');
      expect(toggle?.getAttribute('aria-expanded')).toBe('true');
      act(() => {
        toggle?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
        toggle?.click();
      });
      expect(actions).toEqual([`collapse:worktree/${lane.id}`]);
      // ProjectHead renders org avatar and wraps children in ProjectSaidHere so WorktreeChip omits Northwind prefix.
      const projHead = openView.el.querySelector('#proj-open .ca-grouphead.is-project-head');
      expect(projHead?.querySelector('.ca-grouphead-name')?.textContent).toBe('Northwind');
      expect(projHead?.querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON_N);
      const childChip = openView.el.querySelector('#proj-open .ca-group-body .ca-wtchip');
      expect(childChip?.textContent).toBe('web');
      expect(childChip?.querySelector('img.ca-projicon')).toBeNull();
      // Clicking a ProjectHead toggles collapse.
      act(() => {
        (projHead as HTMLElement)?.click();
      });
      expect(actions).toEqual([`collapse:worktree/${lane.id}`, 'collapse:project/Northwind']);
      const repoMain = fx.lane({ id: '/r/atlas', project: 'Atlas', repo: 'atlas', callsign: 'atlas', origin: 'main', reuse: 'open' });
      const repoSlot = fx.lane({ id: '/r/ci-landing', project: 'Atlas', repo: 'atlas', callsign: 'ci-landing', origin: 'claude-app', reuse: 'open' });
      const repoView = mount(
        <>
          <div id="repo-main">
            <AgentRow lane={repoMain} session={session} domId="row-rm" now={fx.NOW} slim selected={false} onSelect={() => {}} onOpen={() => {}} inRepo />
          </div>
          <div id="repo-slot">
            <AgentRow lane={repoSlot} session={session} domId="row-rs" now={fx.NOW} slim selected={false} onSelect={() => {}} onOpen={() => {}} inRepo />
          </div>
        </>,
      );
      try {
        expect(repoView.el.querySelector('#repo-main .ca-wtchip')).toBeNull();
        expect(repoView.el.querySelector('#repo-slot .ca-wtchip-name')?.textContent).toBe('ci-landing');
      } finally {
        repoView.unmount();
      }
    } finally {
      openView.unmount();
    }
  });
});
