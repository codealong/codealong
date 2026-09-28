// The editors a worktree's folder opens in (editors.rs): yours, which `l|editor` opens, and the
// others installed on this Mac. Picking another makes it yours, as Codex's Open button does.

import { useEffect, useState } from 'react';
import { editorIcons } from './api';
import type { MenuImage, MenuSpec } from './contextMenu';
import type { Board, EditorIcon, Lane, Profile, Target } from './types';

type Apps = Pick<Board['apps'], 'editor' | 'editorId' | 'editors'>;

/** An editor's app icon, as a page shows it and as a native menu takes it. */
export interface AppIcon {
  url: string;
  menu: MenuImage;
}

export type AppIcons = Record<string, AppIcon>;

function appIcon(i: EditorIcon): AppIcon {
  const menu = { rgba: i.rgba, width: i.size, height: i.size };
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = i.size;
  const ctx = canvas.getContext('2d');
  if (!ctx) return { url: '', menu };
  ctx.putImageData(new ImageData(new Uint8ClampedArray(i.rgba), i.size, i.size), 0, 0);
  return { url: canvas.toDataURL(), menu };
}

// One ask a window, again only when the editors installed change.
let asked: { ids: string; icons: Promise<AppIcons> } | null = null;
let latest: AppIcons = {};

function loadIcons(ids: string): Promise<AppIcons> {
  if (asked?.ids !== ids) {
    const icons = editorIcons()
      .then((list) => Object.fromEntries(list.map((i) => [i.id, appIcon(i)])))
      .catch(() => ({}));
    asked = { ids, icons };
  }
  return asked.icons;
}

/** The installed editors' icons by id: none until the engine has drawn them. */
export function useEditorIcons(apps: Pick<Apps, 'editors'> | undefined): AppIcons {
  const ids = (apps?.editors ?? []).map((e) => e.id).join(',');
  const [icons, setIcons] = useState<AppIcons>(latest);
  useEffect(() => {
    if (!ids) return;
    let live = true;
    void loadIcons(ids).then((i) => {
      latest = i;
      if (live) setIcons(i);
    });
    return () => {
      live = false;
    };
  }, [ids]);
  return icons;
}

/** The target that opens a worktree's folder in the editor `id`. */
export const editorTarget = (id: string) => `l|editor|${id}`;

/** Whether the worktree's folder is on this Mac, where an editor can open it. */
export const opensInEditor = (lane: Lane) => !lane.device && lane.targets.some((t) => t.id === 'l|editor');

/** The editors installed here besides yours, in menu order. */
export function otherEditors(apps: Apps): Profile[] {
  return (apps.editors ?? []).filter((e) => e.id !== apps.editorId);
}

/** Yours first, then the others. */
export function allEditors(apps: Apps): Profile[] {
  return [{ id: apps.editorId, name: apps.editor }, ...otherEditors(apps)].filter((e) => e.id);
}

function target(e: Profile, words: string, icons: AppIcons): Target {
  return { id: editorTarget(e.id), icon: 'code', image: icons[e.id]?.url, label: `${words} ${e.name}`, detail: null, kbd: null, disabled: false, group: 'lane' };
}

/** A worktree's targets with the other editors after Open in <yours>, for its Open menu, each
 *  with its app's icon once there are icons. */
export function withOtherEditors(targets: Target[], apps: Apps, icons: AppIcons = {}): Target[] {
  const at = targets.findIndex((t) => t.id === 'l|editor');
  const more = otherEditors(apps);
  if (at < 0 || more.length === 0) return targets;
  const yours = { ...targets[at], image: icons[apps.editorId]?.url };
  return [...targets.slice(0, at), yours, ...more.map((e) => target(e, 'Open in', icons)), ...targets.slice(at + 1)];
}

/** Every editor as Open folder in <editor>, for a session's Open menu, whose other targets open
 *  the session itself. */
export function folderTargets(apps: Apps, icons: AppIcons = {}): Target[] {
  return allEditors(apps).map((e) => target(e, 'Open folder in', icons));
}

/** A worktree's Open in submenu, after Open in <yours>: the other editors. `open` takes a target. */
export function openInMenu(apps: Apps, icons: AppIcons, open: (target: string) => void): MenuSpec {
  const others = otherEditors(apps);
  if (others.length === 0) return [];
  return [{ label: 'Open in', items: others.map((e) => ({ label: e.name, icon: icons[e.id]?.menu, run: () => open(editorTarget(e.id)) })) }];
}

/** A session's menu opens its worktree's folder: in yours, or with others installed, from a
 *  submenu of all of them. */
export function folderMenu(apps: Apps, icons: AppIcons, open: (target: string) => void): MenuSpec {
  const all = allEditors(apps);
  if (all.length === 0) return [];
  if (all.length === 1) return [{ label: `Open folder in ${all[0].name}`, run: () => open('l|editor') }];
  return [{ label: 'Open folder in', items: all.map((e) => ({ label: e.name, icon: icons[e.id]?.menu, run: () => open(editorTarget(e.id)) })) }];
}
