// @vitest-environment jsdom
// DOM tests for SessionBackdrop, SessionThemePicker and SessionThemeProvider:
// renders nothing when the experiment is off, renders each vibe with deterministic SVG/DOM
// layers when on, lets you pick a color or switch vibes from the header popover, and updates
// Sessions list rows when opted in.

import { describe, expect, it, beforeEach } from 'vitest';
import { createRoot } from 'react-dom/client';
import { act } from '../testing/dom';
import { SESSION_VIBES, writeThemePrefs, DEFAULT_PREFS } from '../sessionTheme';
import { SessionBackdrop, SessionThemePicker } from './SessionBackdrop';
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

function Probe({ session }: { session: Session }) {
  const theme = useSessionTheme(session);
  if (!theme) return <div data-testid="off">off</div>;
  return (
    <div data-testid="on" data-color={theme.assigned.color.id} data-vibe={theme.prefs.vibe} data-surface={theme.prefs.surface}>
      <SessionBackdrop theme={theme.assigned} vibe={theme.prefs.vibe} intensity={theme.prefs.intensity} place="header" working={session.status === 'working'} />
      <SessionThemePicker theme={theme} />
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
});

describe('SessionThemeProvider and SessionBackdrop', () => {
  it('stays completely inactive when experimental.session_themes is off', () => {
    const s = makeSession('a', { emblem: { name: 'Rocket', emoji: '🚀' } });
    const view = mount(
      <SessionThemeProvider board={{ lanes: [makeLane([s])], experimental: { boards: false, session_themes: false } }}>
        <Probe session={s} />
      </SessionThemeProvider>,
    );
    expect(view.el.querySelector('[data-testid="off"]')).not.toBeNull();
    expect(view.el.querySelector('.ca-st-layer')).toBeNull();
    view.unmount();
  });

  it('renders every vibe cleanly and lets the header picker change color and vibe live', () => {
    const s = makeSession('a', { emblem: { name: 'Rocket', emoji: '🚀' } });
    const view = mount(
      <SessionThemeProvider board={{ lanes: [makeLane([s])], experimental: { boards: false, session_themes: true } }}>
        <Probe session={s} />
      </SessionThemeProvider>,
    );
    const on = () => view.el.querySelector('[data-testid="on"]') as HTMLElement;
    expect(on().dataset.color).toBe('indigo');
    expect(view.el.querySelector('.ca-st-layer.is-header.is-mesh')).not.toBeNull();

    // Open the header popover, pick Terracotta, and cycle through every vibe.
    act(() => (view.el.querySelector('.ca-st-swatchbtn') as HTMLElement).click());
    expect(view.el.querySelector('.ca-st-popover')).not.toBeNull();

    act(() => (view.el.querySelector('.ca-st-swatch[aria-label="Terracotta"]') as HTMLElement).click());
    expect(on().dataset.color).toBe('clay');

    for (const v of SESSION_VIBES) {
      const btn = [...view.el.querySelectorAll('.ca-st-pill')].find((b) => b.textContent === v.label) as HTMLElement;
      expect(btn, `vibe pill ${v.label}`).toBeTruthy();
      act(() => btn.click());
      expect(on().dataset.vibe).toBe(v.id);
      expect(view.el.querySelector(`.ca-st-layer.is-${v.id}`)).not.toBeNull();
    }

    // Switch back to Auto restores the emblem-matched Indigo color.
    act(() => (view.el.querySelector('.ca-st-auto') as HTMLElement).click());
    expect(on().dataset.color).toBe('indigo');
    view.unmount();
  });
});
