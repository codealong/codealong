// The native menus a unit test's page asked the engine for (contextMenu.ts `showContextMenu`),
// and a pick from one as the engine tells the page of it. The page's own code runs: Tauri's menu
// API over the engine's stand-in (./tauri), which is asked to make each menu
// (`plugin:menu|new`, with its items) and keeps what the page sent. Each item's action sits behind
// a channel of its own there, which the engine messages when the item is picked.
//
// A test file that imports this has `plugin:menu|new` answered before each test, with the
// resource Tauri's API expects back: without it a menu fails as it's made, as it would in a page
// with no engine. Import it after ./dom (or ./tauri).
//
// Only tests import this file. Nothing the app's pages load reaches it, so it isn't in the bundle.

import { act } from 'react';
import { beforeEach } from 'vitest';
import { engine } from './tauri';

/** A menu's item as Tauri hands it to the engine: its words, or none for a line; the key it
 *  shows; a submenu's items; and the channel a pick is told on. */
export interface AskedItem {
  text?: string;
  enabled?: boolean;
  checked?: boolean;
  accelerator?: string;
  items?: AskedItem[];
  handler?: { onmessage: (id: string) => void };
}

let made = 0;
beforeEach(() => {
  engine.answer('plugin:menu|new', () => [++made, `menu-${made}`]);
});

/** The menus the page asked for since the test began, in order, each as its items. */
export function menusAsked(): AskedItem[][] {
  return engine.sent('plugin:menu|new').map((args) => (args.options as { items?: AskedItem[] } | undefined)?.items ?? []);
}

/** A menu as its words: a submenu as its label and ›, a line as -, a checked item with ✓. */
export function menuWords(items: AskedItem[]): string[] {
  return items.map((i) => (i.text === undefined ? '-' : i.items ? `${i.text} ›` : `${i.checked ? '✓ ' : ''}${i.text}`));
}

/** The items under a submenu of `items`. */
export function under(items: AskedItem[], label: string): AskedItem[] {
  const found = items.find((i) => i.text === label && i.items);
  if (!found?.items) throw new Error(`No ${label} › in ${menuWords(items).join(', ')}`);
  return found.items;
}

/** Picks an item of a menu, as the engine tells the page it was picked. */
export function pick(items: AskedItem[], label: string): void {
  const handler = items.find((i) => i.text === label && !i.items)?.handler;
  if (!handler) throw new Error(`No ${label} in ${menuWords(items).join(', ')}`);
  act(() => handler.onmessage(''));
}
