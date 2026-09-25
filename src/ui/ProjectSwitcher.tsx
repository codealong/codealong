// The app-wide project switcher, top left, like an account or org switcher.

import { useEffect, useRef, useState } from 'react';
import type { LogoStatus, ProjectSummary } from '../types';
import { Icon, Mark, StatusGlyph } from './primitives';
import { LOGO_LABEL } from '../logo';

export const ALL_PROJECTS = 'All projects';

/** What waits for you in a project: agents and PRs that need you, agents that finished. */
export interface ActionCounts {
  needs: number;
  unread: number;
}

interface ProjectSwitcherProps {
  logoStatus: LogoStatus;
  projects: ProjectSummary[];
  current: string;
  total: number;
  actions: Record<string, ActionCounts>;
  onPick: (name: string) => void;
  /** Opens a project in a window of its own. */
  onOpenWindow: (name: string) => void;
  onEditProjects: () => void;
}

export function ProjectSwitcher({ logoStatus, projects, current, total, actions, onPick, onOpenWindow, onEditProjects }: ProjectSwitcherProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);
  const pick = (name: string) => {
    setOpen(false);
    onPick(name);
  };
  // Other projects' action items badge the button, so nothing waits unseen behind the switcher.
  const others = projects.filter((p) => p.name !== current && current !== ALL_PROJECTS);
  const otherNeeds = others.reduce((n, p) => n + (actions[p.name]?.needs ?? 0), 0);
  const otherUnread = others.reduce((n, p) => n + (actions[p.name]?.unread ?? 0), 0);
  const badgeTitle = otherNeeds > 0 ? `${otherNeeds} in other projects need you` : otherUnread > 0 ? `${otherUnread} finished in other projects` : '';
  const item = (name: string, count: number) => {
    const a = actions[name];
    return (
      <div key={name} className="ca-projswitch-row" role="none">
        <button type="button" role="menuitemradio" aria-checked={name === current} className={['ca-menu-item', name === current && 'is-on'].filter(Boolean).join(' ')} onClick={() => pick(name)}>
          <Icon name={name === ALL_PROJECTS ? 'stack' : 'folder'} size={16} />
          <span className="ca-menu-label">
            <span>{name}</span>
          </span>
          {a && a.needs > 0 && (
            <span className="ca-projswitch-action is-needs" title={`${a.needs} need you`}>
              <StatusGlyph status="needs" size={10} title="" />
              {a.needs}
            </span>
          )}
          {a && a.unread > 0 && (
            <span className="ca-projswitch-action is-unread" title={`${a.unread} finished`}>
              <StatusGlyph status="unread" size={10} title="" />
              {a.unread}
            </span>
          )}
          <span className="ca-nav-count" title={`${count} worktrees`}>
            {count}
          </span>
          <span className="ca-projswitch-check">{name === current ? <Icon name="check" size={14} /> : null}</span>
        </button>
        <button
          type="button"
          role="menuitem"
          className="ca-projswitch-newwin"
          aria-label={`Open ${name} in a new window`}
          title={name === current ? 'Open in a new window (⌘⇧N)' : 'Open in a new window'}
          onClick={() => {
            setOpen(false);
            onOpenWindow(name);
          }}
        >
          <Icon name="external" size={14} />
        </button>
      </div>
    );
  };
  return (
    <div className="ca-projswitch" ref={ref}>
      <button
        type="button"
        className="ca-projswitch-btn"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`${current}. Switch project${badgeTitle ? `. ${badgeTitle}` : ''}. Codealong: ${LOGO_LABEL[logoStatus]}`}
        title={`Switch project (⌘P)${badgeTitle ? `. ${badgeTitle}.` : ''} · Codealong: ${LOGO_LABEL[logoStatus]}`}
        onClick={() => setOpen(!open)}
      >
        <Mark size={16} status={logoStatus} />
        <span className="ca-projswitch-name">{current}</span>
        {otherNeeds > 0 ? <span className="ca-projswitch-badge">{otherNeeds}</span> : otherUnread > 0 ? <span className="ca-projswitch-dot" /> : null}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <div className="ca-menu ca-projswitch-menu" role="menu">
          {projects.map((p) => item(p.name, p.lanes))}
          <div className="ca-menu-sep" role="separator" />
          {item(ALL_PROJECTS, total)}
          <div className="ca-menu-sep" role="separator" />
          <button
            type="button"
            role="menuitem"
            className="ca-menu-item"
            onClick={() => {
              setOpen(false);
              onEditProjects();
            }}
          >
            <Icon name="settings" size={16} />
            <span className="ca-menu-label">
              <span>Edit projects…</span>
            </span>
          </button>
        </div>
      )}
    </div>
  );
}

/** Opens the switcher from anywhere: exposed so ⌘P can click it. */
export function openSwitcher() {
  document.querySelector<HTMLButtonElement>('.ca-projswitch-btn')?.click();
}
