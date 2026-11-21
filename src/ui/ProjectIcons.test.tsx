// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act } from '../testing/dom';
import { createRoot } from 'react-dom/client';
import type { ReactNode } from 'react';
import type { ProjectSummary } from '../types';
import { ALL_PROJECTS, ProjectSwitcher } from './ProjectSwitcher';
import { ProjectIcons, projectIconMap, ProjectPrefix, ProjectShown, ProjectTag } from './ProjectSaid';

const ICON = 'data:image/png;base64,iVBORw0KGgo=';

const PROJECTS: ProjectSummary[] = [
  { name: 'Northwind', lanes: 4, repos: [], icon: ICON },
  { name: 'Personal', lanes: 2, repos: [] },
];

function render(node: ReactNode) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() => root.render(node));
  return {
    el,
    unmount: () => {
      act(() => root.unmount());
      el.remove();
    },
  };
}

describe('ProjectSwitcher org icons', () => {
  it('shows the selected org avatar in the button (falling back to Mark) and org avatars in the dropdown', () => {
    const view = render(
      <ProjectSwitcher
        logoStatus="idle"
        projects={PROJECTS}
        current="Northwind"
        total={6}
        actions={{}}
        onPick={() => {}}
        onOpenWindow={() => {}}
        onAddProject={() => {}}
        onEditProjects={() => {}}
      />,
    );
    try {
      const btn = view.el.querySelector<HTMLButtonElement>('.ca-projswitch-btn')!;
      // Selected org project shows its avatar in the button instead of Mark.
      const btnImg = btn.querySelector<HTMLImageElement>('img.ca-projicon');
      expect(btnImg?.getAttribute('src')).toBe(ICON);
      expect(btn.querySelector('.ca-mark')).toBeNull();

      // If the button's org avatar fails to load, it falls back to Codealong's Mark.
      act(() => {
        btnImg!.dispatchEvent(new Event('error', { bubbles: false }));
      });
      expect(btn.querySelector('img.ca-projicon')).toBeNull();
      expect(btn.querySelector('.ca-mark')).not.toBeNull();

      act(() => btn.click());
      const items = [...view.el.querySelectorAll('.ca-projswitch-menu .ca-projswitch-row')];
      // First row (Northwind) renders the org avatar img; second (Personal) falls back to folder icon.
      const northwindImg = items[0].querySelector<HTMLImageElement>('img.ca-projicon');
      expect(northwindImg?.getAttribute('src')).toBe(ICON);
      expect(items[1].querySelector('img.ca-projicon')).toBeNull();
      expect(items[1].querySelector('svg.ca-icon')).not.toBeNull();

      // If the menu image fails to load, it falls back cleanly to the folder icon.
      act(() => {
        northwindImg!.dispatchEvent(new Event('error', { bubbles: false }));
      });
      expect(items[0].querySelector('img.ca-projicon')).toBeNull();
      expect(items[0].querySelector('svg.ca-icon')).not.toBeNull();
    } finally {
      view.unmount();
    }
  });

  it('shows Codealong Mark in the button for All projects and projects without an org avatar', () => {
    for (const current of [ALL_PROJECTS, 'Personal']) {
      const view = render(
        <ProjectSwitcher
          logoStatus="idle"
          projects={PROJECTS}
          current={current}
          total={6}
          actions={{}}
          onPick={() => {}}
          onOpenWindow={() => {}}
          onAddProject={() => {}}
          onEditProjects={() => {}}
        />,
      );
      try {
        const btn = view.el.querySelector<HTMLButtonElement>('.ca-projswitch-btn')!;
        expect(btn.querySelector('.ca-mark')).not.toBeNull();
        expect(btn.querySelector('img.ca-projicon')).toBeNull();
      } finally {
        view.unmount();
      }
    }
  });
});

describe('All projects badges', () => {
  it('shows the org avatar in ProjectPrefix and ProjectTag under All projects', () => {
    const icons = projectIconMap(PROJECTS);
    const view = render(
      <ProjectShown.Provider value={ALL_PROJECTS}>
        <ProjectIcons.Provider value={icons}>
          <ProjectPrefix project="Northwind" beside={['app']} />
          <ProjectPrefix project="Personal" beside={['notes']} />
          <ProjectTag project="Northwind" />
          <ProjectTag project="Personal" />
        </ProjectIcons.Provider>
      </ProjectShown.Provider>,
    );
    try {
      const prefixes = [...view.el.querySelectorAll('.ca-projprefix')];
      expect(prefixes).toHaveLength(2);
      expect(prefixes[0].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON);
      expect(prefixes[1].querySelector('img.ca-projicon')).toBeNull();

      const tags = [...view.el.querySelectorAll('.ca-tag')];
      expect(tags).toHaveLength(2);
      expect(tags[0].querySelector('img.ca-projicon')?.getAttribute('src')).toBe(ICON);
      expect(tags[1].querySelector('img.ca-projicon')).toBeNull();
    } finally {
      view.unmount();
    }
  });
});
