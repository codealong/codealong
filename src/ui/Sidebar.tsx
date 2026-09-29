// The sidebar: what the window shows and what narrows it. The view, the devices, how recently
// things were active, their statuses and views you saved, above Settings. Devices and statuses
// toggle, any number of each: none picked shows all. Collapsed it's a rail of the views and
// devices, whose filter button carries a badge while a filter narrows the view. The project
// stays in the switcher at top left.

import { useState, type ReactNode } from 'react';
import type { DeviceFilter } from '../devices';
import { agoPhrase } from '../format';
import type { Filter } from '../lanes';
import type { SavedView } from '../savedViews';
import type { DevicesView, Label, Status } from '../types';
import { cx, Icon, NavItem, StatusGlyph } from './primitives';
import { LabelDot } from './TodoBits';
import { RestartToUpdate } from './Updates';

export interface SideView {
  id: string;
  label: string;
  icon: string;
  count: number | string;
  /** What asks for you there (PRs waiting on your review), in a solid badge. */
  badge?: number;
  badgeTitle?: string;
}

export interface SideAge {
  id: string;
  label: string;
  title: string;
}

export interface SideStatus {
  id: Filter;
  label: string;
  glyph?: Status;
  icon?: string;
  count: number;
  tone?: 'attn' | 'alert';
  title?: string;
}

interface SidebarProps {
  collapsed: boolean;
  onExpand: () => void;
  now: number;
  views: SideView[];
  view: string;
  onView: (id: string) => void;
  /** Linked devices; the device filter shows only with at least one. */
  devices: DevicesView | null | undefined;
  /** The devices picked: `here` for this one, the others by short id. */
  deviceFilter: DeviceFilter;
  /** Picks a device, or unpicks it. */
  onDevice: (id: string) => void;
  onClearDevices: () => void;
  deviceNeeds: Map<string | null, number>;
  deviceWorking: Map<string | null, number>;
  /** Group by device, offered in Worktrees across devices; null where it isn't. */
  groupByDevice: boolean | null;
  onGroupByDevice: () => void;
  ages: SideAge[];
  age: string;
  /** Todo and Canvas show everything, whatever its age. */
  ageApplies: boolean;
  onAge: (id: string) => void;
  statuses: SideStatus[];
  filter: readonly Filter[];
  /** Picks a status, or unpicks it. */
  onFilter: (f: Filter) => void;
  onClearFilters: () => void;
  /** The label Todo is narrowed to, if any. */
  label: Label | null;
  onClearLabel: () => void;
  saved: SavedView[];
  /** The saved view the window shows, if it shows one. */
  savedOn: string | null;
  onApplySaved: (v: SavedView) => void;
  onSavedMenu: (v: SavedView) => void;
  /** A name for the view as it is, offered when you save it. */
  suggestion: string;
  onSave: (name: string) => void;
  /** The filters narrowing the view (age, status, label), for the rail's badge and tooltip. */
  filters: string[];
  onSettings: () => void;
  /** Settings and the theme switch, at the foot of the open sidebar. */
  foot: ReactNode;
  warnings: ReactNode;
  /** The rail marks warnings (config.toml that doesn't parse, say) and opens the sidebar on them. */
  hasWarnings: boolean;
}

export function Sidebar(props: SidebarProps) {
  return props.collapsed ? <Rail {...props} /> : <Open {...props} />;
}

function Open(p: SidebarProps) {
  const [naming, setNaming] = useState<string | null>(null);
  const showDevices = !!p.devices?.enabled && p.devices.devices.length > 0;
  const save = () => {
    if (naming === null) return;
    p.onSave(naming.trim() || p.suggestion);
    setNaming(null);
  };
  return (
    <nav className="ca-side" aria-label="Sidebar">
      <div className="ca-side-scroll">
        <div className="ca-label ca-side-label">View</div>
        {p.views.map((v) => (
          <NavItem key={v.id} icon={v.icon} label={v.label} count={v.count === '' ? undefined : (v.count as number)} badge={v.badge} badgeTitle={v.badgeTitle} active={p.view === v.id} title="Switch views (V)" onClick={() => p.onView(v.id)} />
        ))}
        {showDevices && p.devices && (
          <>
            <SideLabel label="Devices" onClear={p.deviceFilter.length > 0 ? p.onClearDevices : undefined} />
            <DeviceRow
              lead={<Icon name="device-desktop" size={14} />}
              name={p.devices.me.name || 'This device'}
              sub={`This Mac · ${p.deviceWorking.get(null) ?? 0} working`}
              needs={p.deviceNeeds.get(null) ?? 0}
              active={p.deviceFilter.includes('here')}
              onClick={() => p.onDevice('here')}
            />
            {p.devices.devices.map((d) => (
              <DeviceRow
                key={d.key}
                lead={<i className={`ca-labeldot ca-side-devdot is-${d.color}`} aria-hidden="true" />}
                name={d.name}
                sub={d.online ? `${p.deviceWorking.get(d.short) ?? 0} working` : d.lastSeen ? `Away · seen ${agoPhrase(d.lastSeen, p.now)}` : 'Away'}
                title={d.online ? `Connected ${d.via === 'iroh' ? 'through iroh' : 'on your tailnet'}` : undefined}
                away={!d.online}
                needs={p.deviceNeeds.get(d.short) ?? 0}
                active={p.deviceFilter.includes(d.short)}
                onClick={() => p.onDevice(d.short)}
              />
            ))}
            {p.devices.phones.map((d) => (
              <DeviceRow
                key={d.key}
                lead={<Icon name="device-mobile" size={14} />}
                name={d.name}
                sub={d.online ? 'Watches' : d.lastSeen ? `Watches · seen ${agoPhrase(d.lastSeen, p.now)}` : 'Watches'}
                title="Phones and tablets watch; they run no sessions of their own"
                away={!d.online}
              />
            ))}
            {p.groupByDevice !== null && (
              <button type="button" className="ca-nav ca-side-switch" role="switch" aria-checked={p.groupByDevice} title="Group worktrees by the device they're on" onClick={p.onGroupByDevice}>
                <Icon name="layout-grid" size={14} />
                <span className="ca-nav-label">Group by device</span>
                <span className={cx('ca-switch', p.groupByDevice && 'is-on')} aria-hidden="true" />
              </button>
            )}
          </>
        )}
        <div className="ca-label ca-side-label">Active in</div>
        <div className={cx('ca-segctl ca-side-ages', !p.ageApplies && 'is-off')} role="radiogroup" aria-label="Active in" title={p.ageApplies ? undefined : 'This view shows everything, whatever its age'}>
          {p.ages.map((a) => (
            <button key={a.id} type="button" role="radio" aria-checked={p.age === a.id} className={p.age === a.id ? 'is-on' : undefined} title={p.ageApplies ? a.title : undefined} disabled={!p.ageApplies} onClick={() => p.onAge(a.id)}>
              {a.label}
            </button>
          ))}
        </div>
        <SideLabel label="Status" onClear={p.filter.length > 0 ? p.onClearFilters : undefined} />
        <div className="ca-side-chips">
          {p.statuses.map((s) => (
            <button key={s.id} type="button" className={cx('ca-side-chip', p.filter.includes(s.id) && 'is-on')} aria-pressed={p.filter.includes(s.id)} title={s.title} onClick={() => p.onFilter(s.id)}>
              {s.glyph ? <StatusGlyph status={s.glyph} size={9} title="" /> : s.icon ? <Icon name={s.icon} size={12} /> : null}
              <span>{s.label}</span>
              <span className={cx('ca-side-chip-count', s.tone && s.count > 0 && `is-${s.tone}`)}>{s.count}</span>
            </button>
          ))}
          {p.label && (
            <button type="button" className="ca-side-chip is-on" aria-label={`Label ${p.label.name}: show every label`} title="Show every label" onClick={p.onClearLabel}>
              <LabelDot label={p.label} />
              <span>{p.label.name}</span>
              <Icon name="x" size={10} stroke={2.25} />
            </button>
          )}
        </div>
        <div className="ca-label ca-side-label">Saved views</div>
        {p.saved.map((v) => (
          <NavItem key={v.id} icon="star" label={v.name} active={p.savedOn === v.id} title="Right-click to update or delete it" onClick={() => p.onApplySaved(v)} onContextMenu={() => p.onSavedMenu(v)} />
        ))}
        {naming !== null ? (
          <input
            className="ca-side-name"
            autoFocus
            value={naming}
            placeholder={p.suggestion}
            aria-label="Name this view"
            spellCheck={false}
            onChange={(e) => setNaming(e.target.value)}
            onBlur={() => setNaming(null)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                save();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                setNaming(null);
              }
            }}
          />
        ) : (
          p.savedOn === null && (
            <button type="button" className="ca-nav ca-side-save" onClick={() => setNaming(p.suggestion)}>
              <Icon name="plus" size={14} />
              <span className="ca-nav-label">Save this view</span>
            </button>
          )
        )}
      </div>
      <RestartToUpdate />
      {p.foot}
      {p.warnings}
    </nav>
  );
}

/** A section's heading, with a way to unpick everything picked in it. */
function SideLabel({ label, onClear }: { label: string; onClear?: () => void }) {
  return (
    <div className="ca-label ca-side-label ca-side-label--row">
      <span>{label}</span>
      {onClear && (
        <button type="button" className="ca-side-clear" onClick={onClear}>
          Clear
        </button>
      )}
    </div>
  );
}

function DeviceRow({ lead, name, sub, title, needs, away, active, onClick }: { lead: ReactNode; name: string; sub: string; title?: string; needs?: number; away?: boolean; active?: boolean; onClick?: () => void }) {
  const body = (
    <>
      {lead}
      <span className="ca-side-dev">
        <span className="ca-side-dev-name">{name}</span>
        <span className="ca-side-dev-sub">{sub}</span>
      </span>
      {needs ? <span className="ca-nav-badge">{needs}</span> : null}
    </>
  );
  if (!onClick) {
    return (
      <div className={cx('ca-nav ca-side-devrow is-static', away && 'is-away')} title={title}>
        {body}
      </div>
    );
  }
  return (
    <button type="button" className={cx('ca-nav ca-side-devrow', active && 'is-active', away && 'is-away')} aria-pressed={active} title={title} onClick={onClick}>
      {body}
    </button>
  );
}

function Rail(p: SidebarProps) {
  const showDevices = !!p.devices?.enabled && p.devices.devices.length > 0;
  const needs = p.statuses.find((s) => s.id === 'needs')?.count ?? 0;
  const filtered = p.filters.length > 0;
  return (
    <nav className="ca-rail" aria-label="Sidebar">
      {p.views.map((v) => (
        <RailButton key={v.id} icon={v.icon} label={v.id === 'prs' && v.badge ? `${v.label} · ${v.badgeTitle}` : v.label} active={p.view === v.id} badge={v.id === 'sessions' ? needs : v.badge} onClick={() => p.onView(v.id)} />
      ))}
      {showDevices && p.devices && (
        <>
          <span className="ca-rail-sep" aria-hidden="true" />
          <RailButton icon="device-desktop" label={`${p.devices.me.name || 'This device'} · this Mac`} active={p.deviceFilter.includes('here')} badge={p.deviceNeeds.get(null)} onClick={() => p.onDevice('here')} />
          {p.devices.devices.map((d) => (
            <RailButton
              key={d.key}
              icon="device-desktop"
              dot={d.color}
              away={!d.online}
              label={d.online ? d.name : `${d.name} · away`}
              active={p.deviceFilter.includes(d.short)}
              badge={p.deviceNeeds.get(d.short)}
              onClick={() => p.onDevice(d.short)}
            />
          ))}
        </>
      )}
      <span className="ca-rail-sep" aria-hidden="true" />
      <RailButton icon="filter" label={filtered ? `Filtered: ${p.filters.join(', ')}. Show the sidebar` : 'Filters: show the sidebar'} active={filtered} badge={p.filters.length} tone="ink" onClick={p.onExpand} />
      <span className="ca-rail-fill" />
      {p.hasWarnings && <RailButton icon="alert" label="Warnings: show the sidebar" alert onClick={p.onExpand} />}
      <RestartToUpdate compact />
      <RailButton icon="settings" label="Settings (⌘,)" onClick={p.onSettings} />
    </nav>
  );
}

function RailButton({ icon, label, active, badge, tone = 'attn', dot, away, alert, onClick }: { icon: string; label: string; active?: boolean; badge?: number; tone?: 'attn' | 'ink'; dot?: string; away?: boolean; alert?: boolean; onClick: () => void }) {
  return (
    <button type="button" className={cx('ca-rail-btn', active && 'is-on', away && 'is-away', alert && 'is-alert')} aria-label={label} title={label} aria-pressed={active} onClick={onClick}>
      <Icon name={icon} size={18} />
      {dot && <i className={`ca-labeldot ca-rail-dot is-${dot}`} aria-hidden="true" />}
      {badge ? <span className={`ca-rail-badge is-${tone}`}>{badge}</span> : null}
    </button>
  );
}
