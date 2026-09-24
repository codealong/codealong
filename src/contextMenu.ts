// Native context menus (right-click), through Tauri's menu API. Outside the app, in a browser
// preview, there's no native menu and right-click does nothing.

import { Menu } from '@tauri-apps/api/menu';
import { inTauri } from './api';

export interface MenuAction {
  label: string;
  run: () => void;
  enabled?: boolean;
}

export type MenuSpec = (MenuAction | 'separator')[];

/** Shows a native menu at the pointer. */
export async function showContextMenu(items: MenuSpec): Promise<void> {
  if (!inTauri || items.length === 0) return;
  const menu = await Menu.new({
    items: items.map((it, i) =>
      it === 'separator' ? { item: 'Separator' as const } : { id: `ctx-${i}`, text: it.label, enabled: it.enabled ?? true, action: () => it.run() },
    ),
  });
  try {
    await menu.popup();
  } finally {
    await menu.close();
  }
}
