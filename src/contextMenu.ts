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

// A right-click doesn't select a list's row, so the row a menu is for is ringed while the menu
// is up, as the Finder rings a file (`[data-menu]` in LaneRow.css). The row is the one the click
// that asked for the menu landed on: a session's line on its worktree's row, else the row.
// It's marked with an attribute, which React leaves alone when the row renders again.
let pressed: Element | null = null;
// What had the keyboard when the pointer was last pressed, before the press could move it: by
// the time a right-click's menu event comes, a text field under it can have the keyboard, and on
// macOS a word in it selected.
let focusAtPress: Element | null = null;
if (typeof document !== 'undefined') {
  document.addEventListener(
    'contextmenu',
    (e) => {
      pressed = e.target instanceof Element ? e.target.closest('[role="option"], .ca-lane') : null;
      // Only for a menu asked for while this click is handled, not one a key or a button opens later.
      setTimeout(() => {
        pressed = null;
      }, 0);
    },
    true,
  );
  document.addEventListener(
    'mousedown',
    () => {
      focusAtPress = document.activeElement;
    },
    true,
  );
}

const FIELD = 'input, textarea, [contenteditable="true"]';

/** The text field a right-click landed in, if any. */
export function fieldAt(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? target.closest<HTMLElement>(FIELD) : null;
}

/** Whether a right-click landed in a text field you were typing in: that keeps its own menu, for
 *  copy, paste and spelling. One you weren't typing in is part of what it's on (`headerMenu`). */
export function typingIn(target: EventTarget | null): boolean {
  const field = fieldAt(target);
  return !!field && field === focusAtPress;
}

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
  const row = at ? null : pressed;
  row?.setAttribute('data-menu', '');
  try {
    const menu = await Menu.new({ items: options(items, 'ctx') });
    try {
      // Not menu.popup(): Tauri's keeps a lock the next right-click's Menu.new needs while the
      // menu is open, which hung the app (popup_menu in lib.rs).
      await popupMenu(menu.rid, at);
    } finally {
      await menu.close();
    }
  } finally {
    row?.removeAttribute('data-menu');
  }
}

/** Where to drop a menu from a button: its bottom left corner. */
export function below(el: Element): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 4 };
}
