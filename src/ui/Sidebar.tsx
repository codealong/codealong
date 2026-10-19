// The sidebar: what the window shows and what narrows it. The view, the devices, how recently
// things were active, their statuses, what else the view narrows by (`SideFacetChips`: agents,
// repos, labels, authors) and views you saved, above Settings. How Sessions, PRs and Todo are sorted, and how
// much a row says, is the bar over the list (ListBar.tsx `ListTools`). Devices, statuses and
// facets toggle, any number of each: none picked shows all. Collapsed,
// as it starts, it's a rail of the views and devices, whose filter button carries a badge while a
// filter narrows the view. The project stays in the switcher at top left.

import { useState, type CSSProperties, type ReactNode } from 'react';
import type { FacetId, FacetOption } from '../boardLists';
import type { DeviceFilter } from '../devices';
import { agoPhrase } from '../format';
import type { Filter } from '../lanes';
import type { SavedView } from '../savedViews';
import type { DevicesView, LabelColor, Status } from '../types';
import { cx, Icon, NavItem, Seg, StatusGlyph } from './primitives';
import { LabelDot } from './TodoBits';
import { RestartToUpdate } from './Updates';
import { joinKeys, keysIn } from '../platform';

export interface SideView {
  id: string;
  label: string;
  icon: string;
  count: number | string;
  /** What asks for you there (PRs waiting on your review), in a solid badge. */
  badge?: number;
  badgeTitle?: string;
  /** The keys that go to it (`mainKeys.ts` `VIEW_CHORDS`), one sign at a time: ⇧ ⌘ S. */
  kbd: string[];
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
  /** What else narrows the view showing (boardLists.ts `facetsIn`): agents, repos, labels. */
  facets: SideFacet[];
  /** Picks one of a facet's options, or unpicks it. */
  onFacet: (id: FacetId, pick: string) => void;
  onClearFacet: (id: FacetId) => void;
  saved: SavedView[];
  /** The saved view the window shows, if it shows one. */
  savedOn: string | null;
  onApplySaved: (v: SavedView) => void;
  onSavedMenu: (v: SavedView) => void;
  /** A name for the view as it is, offered when you save it. */
  suggestion: string;
  onSave: (name: string) => void;
  /** The filters narrowing the view (age, statuses, facets), for the rail's badge and tooltip. */
  filters: string[];
  onSettings: () => void;
  /** Settings and the theme switch, at the foot of the open sidebar. */
  foot: ReactNode;
  /** This Mac's board hasn't been read yet: no counts show, since they'd be other devices' alone. */
  loading: boolean;
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
          <NavItem key={v.id} icon={v.icon} label={v.label} count={v.count === '' || p.loading ? undefined : (v.count as number)} badge={v.badge} badgeTitle={v.badgeTitle} kbd={v.kbd} active={p.view === v.id} title={keysIn('⌃Tab goes to the next view')} onClick={() => p.onView(v.id)} />
        ))}
        {showDevices && p.devices && (
          <>
            <SideLabel label="Devices" onClear={p.deviceFilter.length > 0 ? p.onClearDevices : undefined} />
            <DeviceRow
              lead={<Icon name="device-desktop" size={14} />}
              name={p.devices.me.name || 'This device'}
              sub={p.loading ? 'This Mac · reading sessions…' : `This Mac · ${p.deviceWorking.get(null) ?? 0} working`}
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
          </>
        )}
        <div className="ca-label ca-side-label">Active in</div>
        <Seg
          className={cx('ca-side-ages', !p.ageApplies && 'is-off')}
          label="Active in"
          title={p.ageApplies ? undefined : 'This view shows everything, whatever its age'}
          options={p.ageApplies ? p.ages : p.ages.map((a) => ({ ...a, title: undefined }))}
          value={p.age}
          disabled={!p.ageApplies}
          onPick={p.onAge}
        />
        <SideLabel label="Status" onClear={p.filter.length > 0 ? p.onClearFilters : undefined} />
        <div className="ca-side-chips">
          {p.statuses.map((s) => (
            <button key={s.id} type="button" className={cx('ca-side-chip', p.filter.includes(s.id) && 'is-on')} aria-pressed={p.filter.includes(s.id)} title={s.title} onClick={() => p.onFilter(s.id)}>
              {s.glyph ? <StatusGlyph status={s.glyph} size={9} title="" /> : s.icon ? <Icon name={s.icon} size={12} /> : null}
              <span>{s.label}</span>
              {!p.loading && <span className={cx('ca-side-chip-count', s.tone && s.count > 0 && `is-${s.tone}`)}>{s.count}</span>}
            </button>
          ))}
        </div>
        {p.facets.map((f) => (
          <SideFacetChips key={f.id} facet={f} loading={p.loading} onPick={(pick) => p.onFacet(f.id, pick)} onClear={() => p.onClearFacet(f.id)} />
        ))}
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
    </nav>
  );
}

export interface SideFacet {
  id: FacetId;
  label: string;
  options: FacetOption[];
  picked: readonly string[];
}

/** How many of a facet's options show before "more": the rest are a click away. */
const FACET_SHOWN = 8;

/** A facet's options as the statuses are, toggles with counts, each with what marks it: a repo's
 *  book, a label's dot in its color, an author's head. A long list shows its first few, the picked ones always. */
function SideFacetChips({ facet: f, loading, onPick, onClear }: { facet: SideFacet; loading: boolean; onPick: (pick: string) => void; onClear: () => void }) {
  const [all, setAll] = useState(false);
  if (f.options.length === 0) return null;
  const shown = all ? f.options : f.options.filter((o, i) => i < FACET_SHOWN || f.picked.includes(o.id));
  const more = f.options.length - shown.length;
  return (
    <>
      <SideLabel label={f.label} onClear={f.picked.length > 0 ? onClear : undefined} />
      <div className="ca-side-chips">
        {shown.map((o) => {
          const on = f.picked.includes(o.id);
          return (
            <button key={o.id} type="button" className={cx('ca-side-chip', on && 'is-on')} aria-pressed={on} title={o.title} onClick={() => onPick(o.id)}>
              {f.id === 'repos' && <Icon name="book" size={12} />}
              {f.id === 'authors' && <Icon name="user" size={12} />}
              {f.id === 'prLabels' && <i className="ca-labeldot ca-prlabel-dot" style={o.color ? ({ '--prlabel': `#${o.color}` } as CSSProperties) : undefined} aria-hidden="true" />}
              {f.id === 'labels' && <LabelDot label={{ id: o.id, name: o.label, color: (o.color ?? 'gray') as LabelColor }} />}
              <span>{o.label}</span>
              {!loading && <span className="ca-side-chip-count">{o.count}</span>}
            </button>
          );
        })}
        {(more > 0 || all) && (
          <button type="button" className="ca-linkbtn ca-side-more" onClick={() => setAll(!all)}>
            {all ? 'Fewer' : `${more} more`}
          </button>
        )}
      </div>
    </>
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
  const needs = p.loading ? 0 : (p.statuses.find((s) => s.id === 'needs')?.count ?? 0);
  const filtered = p.filters.length > 0;
  return (
    <nav className="ca-rail" aria-label="Sidebar">
      {p.views.map((v) => (
        <RailButton key={v.id} icon={v.icon} label={`${v.label} (${joinKeys(v.kbd)})${v.id === 'prs' && v.badge ? ` · ${v.badgeTitle}` : ''}`} active={p.view === v.id} badge={v.id === 'sessions' ? needs : v.badge} onClick={() => p.onView(v.id)} />
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
      <RestartToUpdate compact />
      <RailButton icon="settings" label={keysIn('Settings (⌘,)')} onClick={p.onSettings} />
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
