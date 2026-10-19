// Saved views: a view and what narrows it, under a name you gave it, listed in the sidebar. They
// are a convenience of this Mac's windows, so they live in the page's storage, not config.toml.

import type { Facets, FacetId } from './boardLists';
import type { DeviceFilter } from './devices';
import type { Filter } from './lanes';

/** What a window shows: its view, project, devices, how recently active, statuses, and the
 *  sidebar's other picks (agents, repos, labels). A view saved before those were has none. */
export interface ViewState {
  view: string;
  project: string;
  device: DeviceFilter;
  age: string;
  filter: readonly Filter[];
  facets?: Facets;
}

export interface SavedView extends ViewState {
  id: string;
  name: string;
}

const KEY = 'codealong.savedViews';

const text = (v: unknown): v is string => typeof v === 'string';
/** A list of picks: saved before several could be picked, it was one or none. */
const picks = (v: unknown): string[] | null => (v === null ? [] : typeof v === 'string' ? [v] : Array.isArray(v) && v.every(text) ? v : null);

/** The saved views in stored text; anything that doesn't read as one is left out. */
export function parseSaved(raw: string | null): SavedView[] {
  if (!raw) return [];
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(v)) return [];
  return v.flatMap((x: Record<string, unknown> | null): SavedView[] => {
    if (!x || typeof x !== 'object') return [];
    const { id, name, view, project, age } = x;
    const device = picks(x.device);
    const filter = picks(x.filter);
    if (!text(id) || !text(name) || !text(view) || !text(project) || !device || !text(age) || !filter) return [];
    const facets = facetsIn(x.facets);
    return [{ id, name, view, project, device, age, filter: filter as Filter[], ...(facets && { facets }) }];
  });
}

export function loadSaved(): SavedView[] {
  try {
    return parseSaved(window.localStorage.getItem(KEY));
  } catch {
    return [];
  }
}

export function storeSaved(views: SavedView[]): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(views));
  } catch {
    // Storage can be unavailable; the views then last until the window closes.
  }
}

/** Calls back with the saved views when another window changes them. */
export function onSavedChange(f: (views: SavedView[]) => void): () => void {
  const listen = (e: StorageEvent) => {
    if (e.key === KEY) f(parseSaved(e.newValue));
  };
  window.addEventListener('storage', listen);
  return () => window.removeEventListener('storage', listen);
}

/** A saved view's facets: each a list of picks; what doesn't read as one is left out. */
function facetsIn(v: unknown): Facets | null {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return null;
  const out: Facets = {};
  for (const [k, p] of Object.entries(v)) if (Array.isArray(p) && p.every(text) && p.length > 0) out[k as FacetId] = p;
  return Object.keys(out).length > 0 ? out : null;
}

/** Whether two states show the same thing: the same picks, in whatever order. */
export function sameView(a: ViewState, b: ViewState): boolean {
  const same = (x: readonly string[], y: readonly string[]) => x.length === y.length && x.every((v) => y.includes(v));
  const ids = new Set([...Object.keys(a.facets ?? {}), ...Object.keys(b.facets ?? {})]) as Set<FacetId>;
  const facets = [...ids].every((id) => same(a.facets?.[id] ?? [], b.facets?.[id] ?? []));
  return a.view === b.view && a.project === b.project && same(a.device, b.device) && a.age === b.age && same(a.filter, b.filter) && facets;
}

/** A name for a view as it is, offered when you save it: the view, then what narrows it. */
export function suggestName(parts: (string | null | undefined)[]): string {
  return parts.filter((p): p is string => !!p).join(' · ');
}

export function newSavedId(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}
