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
