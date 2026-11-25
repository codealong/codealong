// @vitest-environment jsdom
// DOM & menu tests for SessionBackdrop, SessionThemeProvider, and sessionMenu theme items:
// renders nothing when the experiment is off, cleanly separates Mesh mode (5 grainy mesh vibes +
// Theme color submenu in the session's ⋯/context menu) from AI-generated mode (neutral while
// drawing, artwork fades in with no mesh underneath, and Draw/Redraw background in the menu —
// never both at once).

import { describe, expect, it, beforeEach } from 'vitest';
import { createRoot } from 'react-dom/client';
import { act } from '../testing/dom';
import type { MenuAction, MenuGroup } from '../contextMenu';
import { sessionMenu } from '../menus';
import { SESSION_VIBES, setSessionThemeMode, writeThemePrefs, DEFAULT_PREFS } from '../sessionTheme';
import { SessionBackdrop } from './SessionBackdrop';
import { SessionThemeProvider, useSessionTheme } from './SessionThemeProvider';
import type { Lane, Session } from '../types';

function makeSession(key: string, over: Partial<Session> = {}): Session {
  return {
    key,
    harness: 'claude',
    account: 'claude',
    surface: 'cli',
    title: `Session ${key}`,
    status: 'working',
    lastActivity: 1_000,
    startedAt: 1_000,
    detail: '',
    excerpt: '',
    openTarget: `s|${key}|0`,
    live: true,
    ...over,
  } as Session;
}

function makeLane(sessions: Session[]): Lane {
  return {
    id: 'lane-1',
    project: 'Northwind',
    repo: 'web',
    callsign: 'main',
    path: '/Users/me/web',
    bay: 'working',
    headline: '',
    lastActivity: 1_000,
    targets: [],
    sessions,
  } as unknown as Lane;
}

const dummyCan = {
  report: { info: () => {}, error: () => {} },
  open: () => {},
  terminal: 'Terminal',
};

function Probe({ session }: { session: Session }) {
  const theme = useSessionTheme(session);
  if (!theme) return <div data-testid="off">off</div>;
  return (
    <div data-testid="on" data-color={theme.assigned.color.id} data-vibe={theme.prefs.vibe} data-surface={theme.prefs.surface}>
      <SessionBackdrop theme={theme.assigned} vibe={theme.prefs.vibe} intensity={theme.prefs.intensity} place="panel" working={session.status === 'working'} aiOnly={theme.aiEnabled} />
    </div>
  );
}

function mount(node: React.ReactNode) {
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

beforeEach(() => {
  window.localStorage.clear();
  writeThemePrefs(DEFAULT_PREFS);
  setSessionThemeMode({ enabled: false, aiEnabled: false });
});

describe('SessionThemeProvider, SessionBackdrop, and sessionMenu theme items', () => {
  it('stays completely inactive when experimental.session_themes is off', () => {
    const s = makeSession('a', { emblem: { name: 'Rocket', emoji: '🚀' } });
    const lane = makeLane([s]);
    const view = mount(
      <SessionThemeProvider board={{ lanes: [lane], experimental: { boards: false, session_themes: false } }}>
        <Probe session={s} />
      </SessionThemeProvider>,
    );
    expect(view.el.querySelector('[data-testid="off"]')).not.toBeNull();
    expect(view.el.querySelector('.ca-st-layer')).toBeNull();
    const labels = sessionMenu(lane, s, dummyCan).filter((i): i is MenuAction | MenuGroup => i !== 'separator').map((i) => i.label);
    expect(labels).not.toContain('Theme color');
    expect(labels).not.toContain('Draw background');
    expect(labels).not.toContain('Redraw background');
    view.unmount();
  });

  it('in Mesh mode renders every vibe and puts Theme color (not AI redraw) in the session menu', () => {
    const s = makeSession('a', { emblem: { name: 'Rocket', emoji: '🚀' } });
    const lane = makeLane([s]);
    const view = mount(
      <SessionThemeProvider board={{ lanes: [lane], experimental: { boards: false, session_themes: true } }}>
        <Probe session={s} />
      </SessionThemeProvider>,
    );
    const on = () => view.el.querySelector('[data-testid="on"]') as HTMLElement;
    expect(on().dataset.color).toBe('dusk');
    expect(view.el.querySelector('.ca-st-layer.is-panel.is-grain-mesh .ca-st-canvas')).not.toBeNull();

    const menu = sessionMenu(lane, s, dummyCan);
    const themeGroup = menu.find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === 'Theme color');
    expect(themeGroup).toBeTruthy();
    expect(menu.some((i) => i !== 'separator' && (i.label === 'Draw background' || i.label === 'Redraw background'))).toBe(false);

    // Pick Desert Clay through the menu, then return to Auto.
    const clay = themeGroup!.items.find((i): i is MenuAction => i !== 'separator' && i.label === 'Desert Clay');
    expect(clay).toBeTruthy();
    act(() => clay!.run());
    expect(on().dataset.color).toBe('terracotta');

    const auto = themeGroup!.items.find((i): i is MenuAction => i !== 'separator' && i.label === 'Auto');
    act(() => auto!.run());
    expect(on().dataset.color).toBe('dusk');

    // Every vibe renders cleanly in Mesh mode.
    for (const v of SESSION_VIBES) {
      act(() => writeThemePrefs({ ...DEFAULT_PREFS, vibe: v.id }));
      expect(view.el.querySelector(`.ca-st-layer.is-${v.id}`)).not.toBeNull();
    }
    view.unmount();
  });

  it('in AI mode skips meshes while drawing and puts only Draw/Redraw background in the session menu', () => {
    const pending = makeSession('ai-pending');
    const drawn = makeSession('ai-drawn', {
      backdrop: { picture: 'data:image/png;base64,iVBORw0KGgo=', name: 'Indigo woodblock ridges', accent: '#3b6ea5', luminance: 210 },
    });
    const helper = makeSession('ai-helper', { helper: { report: false, yours: false } });
    const lane = makeLane([pending, drawn, helper]);
    const board = {
      lanes: [lane],
      experimental: { boards: false, session_themes: true },
      backgrounds: { enabled: true, account: 'antigravity', prompt: '' },
    };

    const pendingView = mount(
      <SessionThemeProvider board={board}>
        <Probe session={pending} />
      </SessionThemeProvider>,
    );
    expect(pendingView.el.querySelector('.ca-st-layer')).toBeNull();
    const pendingLabels = sessionMenu(lane, pending, dummyCan).filter((i): i is MenuAction | MenuGroup => i !== 'separator').map((i) => i.label);
    expect(pendingLabels).toContain('Draw background');
    expect(pendingLabels).not.toContain('Theme color');
    pendingView.unmount();

    const drawnView = mount(
      <SessionThemeProvider board={board}>
        <Probe session={drawn} />
      </SessionThemeProvider>,
    );
    expect(drawnView.el.querySelector('.ca-st-layer.is-has-ai.is-light-art .ca-st-ai')).not.toBeNull();
    expect(drawnView.el.querySelector('.ca-st-canvas')).toBeNull();
    const drawnLabels = sessionMenu(lane, drawn, dummyCan).filter((i): i is MenuAction | MenuGroup => i !== 'separator').map((i) => i.label);
    expect(drawnLabels).toContain('Redraw background');
    expect(drawnLabels).not.toContain('Theme color');
    drawnView.unmount();

    // Helpers don't get AI background actions.
    const helperLabels = sessionMenu(lane, helper, dummyCan).filter((i): i is MenuAction | MenuGroup => i !== 'separator').map((i) => i.label);
    expect(helperLabels).not.toContain('Draw background');
    expect(helperLabels).not.toContain('Redraw background');
    expect(helperLabels).not.toContain('Theme color');
  });
});
