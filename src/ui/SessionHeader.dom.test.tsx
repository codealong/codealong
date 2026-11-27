// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act } from '../testing/dom';
import { createRoot } from 'react-dom/client';
import * as fx from '../testing/fixtures';
import { ALL_PROJECTS } from './ProjectSwitcher';
import { ProjectIcons, projectIconMap, ProjectShown } from './ProjectSaid';
import { SessionHeader } from './SessionHeader';

// The header's place (its worktree, after the status) leads with one icon: its project's org
// avatar where the window says the project and has one, else the folder. The two side by side
// said the same thing twice.

const ICON = 'data:image/png;base64,iVBORw0KGgo=';
const ICONS = projectIconMap([
  { name: 'Northwind', icon: ICON },
  { name: 'Atlas', icon: null },
]);

function place(project: string, shown: string | null) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  const lane = fx.lane({ id: '/r/app', project });
  act(() =>
    root.render(
      <ProjectShown.Provider value={shown}>
        <ProjectIcons.Provider value={ICONS}>
          <SessionHeader lane={lane} session={fx.session()} apps={fx.board().apps} now={fx.NOW} onOpen={() => {}} onShowLane={() => {}} />
        </ProjectIcons.Provider>
      </ProjectShown.Provider>,
    ),
  );
  const at = el.querySelector('.ca-sessinsp-lane')!;
  const seen = {
    avatars: [...at.querySelectorAll('img.ca-projicon')].map((img) => img.getAttribute('src')),
    folders: at.querySelectorAll('svg.ca-icon').length,
    project: at.querySelector('.ca-projprefix-name')?.textContent ?? null,
  };
  act(() => root.unmount());
  el.remove();
  return seen;
}

describe('the session header’s place', () => {
  it('leads with the org avatar in the folder’s stead, once', () => {
    expect(place('Northwind', ALL_PROJECTS)).toEqual({ avatars: [ICON], folders: 0, project: 'Northwind' });
  });

  it('keeps the folder for a project without an avatar', () => {
    expect(place('Atlas', ALL_PROJECTS)).toEqual({ avatars: [], folders: 1, project: 'Atlas' });
  });

  it('keeps the folder where the window lists that project and doesn’t say it', () => {
    expect(place('Northwind', 'Northwind')).toEqual({ avatars: [], folders: 1, project: null });
  });
});

describe('the session header’s account limit banner', () => {
  it('shows the warning banner with snooze and move actions when the account is at its limit', () => {
    const el = document.body.appendChild(document.createElement('div'));
    const root = createRoot(el);
    const lane = fx.lane({ id: '/r/app', project: 'Northwind' });
    const session = fx.session({ account: 'claude-work', status: 'idle', limit: undefined });
    const resetsAt = fx.NOW + 9 * 3_600_000;
    let snoozedUntil: number | null = null;
    let moved = false;
    act(() =>
      root.render(
        <SessionHeader
          lane={lane}
          session={session}
          apps={fx.board().apps}
          now={fx.NOW}
          onOpen={() => {}}
          onShowLane={() => {}}
          onMove={() => {
            moved = true;
          }}
          actions={{
            askAside: () => {},
            copyId: { label: 'Copy ID', run: () => {} },
            snoozeUntil: (until) => {
              snoozedUntil = until;
            },
          }}
          accountLimit={{ account: 'claude-work', meter: 'week', resetsAt, spendCapped: false, refused: false }}
        />,
      ),
    );
    const banner = el.querySelector('.ca-sessinsp-limit.is-warn');
    expect(banner).not.toBeNull();
    const words = banner!.querySelector('.ca-sessinsp-limit-words')!;
    expect(words.textContent).toBe('claude-work at weekly limit · lifts in 9h');
    expect(words.getAttribute('title')).toContain('Starting a new turn will hit it unless you move to another account.');
    const buttons = [...banner!.querySelectorAll('button')];
    expect(buttons.map((b) => b.textContent)).toEqual(['Snooze until it lifts', 'Move…']);
    act(() => buttons[0].click());
    expect(snoozedUntil).toBe(resetsAt);
    act(() => buttons[1].click());
    expect(moved).toBe(true);
    act(() => root.unmount());
    el.remove();
  });

  it('does not show the account limit banner when the session already stopped at its own limit or is working', () => {
    const el = document.body.appendChild(document.createElement('div'));
    const root = createRoot(el);
    const lane = fx.lane({ id: '/r/app', project: 'Northwind' });
    const resetsAt = fx.NOW + 3 * 3_600_000;
    const accountLimit = { account: 'claude-work', meter: 'week', resetsAt, spendCapped: false, refused: false };

    act(() =>
      root.render(
        <SessionHeader
          lane={lane}
          session={fx.session({
            account: 'claude-work',
            status: 'error',
            limit: { meter: 'week', resetsAt, at: fx.NOW - 60_000, moveTo: null },
          })}
          apps={fx.board().apps}
          now={fx.NOW}
          onOpen={() => {}}
          accountLimit={accountLimit}
        />,
      ),
    );
    expect(el.querySelectorAll('.ca-sessinsp-limit').length).toBe(1);
    expect(el.querySelector('.ca-sessinsp-limit.is-warn')).toBeNull();

    act(() =>
      root.render(
        <SessionHeader
          lane={lane}
          session={fx.session({ account: 'claude-work', status: 'working' })}
          apps={fx.board().apps}
          now={fx.NOW}
          onOpen={() => {}}
          accountLimit={accountLimit}
        />,
      ),
    );
    expect(el.querySelector('.ca-sessinsp-limit')).toBeNull();

    act(() => root.unmount());
    el.remove();
  });
});
