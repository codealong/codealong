// Native context menus (right-click), through Tauri's menu API. Outside the app, in a browser
// preview, there's no native menu and right-click does nothing.

import { LogicalPosition } from '@tauri-apps/api/dpi';
import { Menu, type MenuOptions } from '@tauri-apps/api/menu';
import { inTauri } from './api';

export interface MenuAction {
  label: string;
  run: () => void;
  enabled?: boolean;
  /** Set to show a checkmark item, checked or not. */
  checked?: boolean;
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
    const item = { id, text: it.label, enabled: it.enabled ?? true, action: () => it.run() };
    return it.checked === undefined ? item : { ...item, checked: it.checked };
  }) as Items;
}

/** Shows a native menu at the pointer, or at `at` (window coordinates) to drop it from a button. */
export async function showContextMenu(items: MenuSpec, at?: { x: number; y: number }): Promise<void> {
  if (!inTauri || items.length === 0) return;
  const menu = await Menu.new({ items: options(items, 'ctx') });
  try {
    await menu.popup(at ? new LogicalPosition(at.x, at.y) : undefined);
  } finally {
    await menu.close();
  }
}

/** Where to drop a menu from a button: its bottom left corner. */
export function below(el: Element): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 4 };
}
