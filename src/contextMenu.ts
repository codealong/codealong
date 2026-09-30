// Native context menus (right-click), through Tauri's menu API. Outside the app, in a browser
// preview, there's no native menu and right-click does nothing.

import { Menu, type MenuOptions } from '@tauri-apps/api/menu';
import { inTauri, popupMenu } from './api';

export interface MenuAction {
  label: string;
  run: () => void;
  enabled?: boolean;
  /** Set to show a checkmark item, checked or not. */
  checked?: boolean;
  /** An image before the label, such as an app's icon. */
  icon?: MenuImage;
  /** A key shown beside the label, for one the window takes for the same action ("E"). */
  accelerator?: string;
}

/** Straight RGBA, top row first, as Tauri takes an image it hasn't loaded (its `JsImage`). */
export interface MenuImage {
  rgba: number[];
  width: number;
  height: number;
}

/** A submenu. One with nothing in it shows dimmed. */
export interface MenuGroup {
  label: string;
  items: MenuSpec;
  enabled?: boolean;
}

export type MenuSpec = (MenuAction | MenuGroup | 'separator')[];

type Items = NonNullable<MenuOptions['items']>;

function options(items: MenuSpec, path: string): Items {
  return items.map((it, i) => {
    const id = `${path}-${i}`;
    if (it === 'separator') return { item: 'Separator' as const };
    if ('items' in it) return { id, text: it.label, enabled: it.enabled ?? it.items.length > 0, items: options(it.items, id) };
    const item = { id, text: it.label, enabled: it.enabled ?? true, action: () => it.run(), ...(it.accelerator ? { accelerator: it.accelerator } : {}) };
    if (it.checked !== undefined) return { ...item, checked: it.checked };
    // Tauri's types list icons it loads itself; raw RGBA goes through to the engine as it is.
    return it.icon ? { ...item, icon: it.icon as unknown as number[] } : item;
  }) as Items;
}

/** Shows a native menu at the pointer, or at `at` (window coordinates) to drop it from a button. */
export async function showContextMenu(items: MenuSpec, at?: { x: number; y: number }): Promise<void> {
  if (!inTauri || items.length === 0) return;
  const menu = await Menu.new({ items: options(items, 'ctx') });
  try {
    // Not menu.popup(): Tauri's keeps a lock the next right-click's Menu.new needs while the
    // menu is open, which hung the app (popup_menu in lib.rs).
    await popupMenu(menu.rid, at);
  } finally {
    await menu.close();
  }
}

/** Where to drop a menu from a button: its bottom left corner. */
export function below(el: Element): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 4 };
}
