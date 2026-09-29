// Saved views: a view and what narrows it, under a name you gave it, listed in the sidebar. They
// are a convenience of this Mac's windows, so they live in the page's storage, not config.toml.

import type { DeviceFilter } from './devices';
import type { Filter } from './lanes';

/** What a window shows: its view, project, device, how recently active, and status. */
export interface ViewState {
  view: string;
  project: string;
  device: DeviceFilter;
  age: string;
  filter: Filter | null;
}

export interface SavedView extends ViewState {
  id: string;
  name: string;
}

const KEY = 'codealong.savedViews';

const text = (v: unknown): v is string => typeof v === 'string';
const orNull = (v: unknown): v is string | null => v === null || typeof v === 'string';

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
    const { id, name, view, project, device, age, filter } = x;
    if (!text(id) || !text(name) || !text(view) || !text(project) || !orNull(device) || !text(age) || !orNull(filter)) return [];
    return [{ id, name, view, project, device, age, filter: filter as Filter | null }];
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

/** Whether two states show the same thing. */
export function sameView(a: ViewState, b: ViewState): boolean {
  return a.view === b.view && a.project === b.project && a.device === b.device && a.age === b.age && a.filter === b.filter;
}

/** A name for a view as it is, offered when you save it: the view, then what narrows it. */
export function suggestName(parts: (string | null | undefined)[]): string {
  return parts.filter((p): p is string => !!p).join(' · ');
}

export function newSavedId(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}
