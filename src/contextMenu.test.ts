import { describe, expect, it } from 'vitest';
import { menuItems, type MenuSpec } from './contextMenu';

describe('menuItems', () => {
  // Tauri drops an item's action by its id as its menu closes: a fixed id would let a menu that
  // closes late take the actions of the one opened after it.
  it('gives no item an id of its own, nested ones included', () => {
    const spec: MenuSpec = [
      { label: 'Open', run: () => {} },
      'separator',
      { label: 'Move to', items: [{ label: 'claude2', run: () => {} }, { label: 'Pinned', run: () => {}, checked: true }] },
    ];
    const ids: unknown[] = [];
    const walk = (items: unknown[]) =>
      items.forEach((item) => {
        if (item && typeof item === 'object') {
          if ('id' in item) ids.push(item.id);
          if ('items' in item && Array.isArray(item.items)) walk(item.items);
        }
      });
    walk(menuItems(spec));
    expect(ids).toEqual([]);
  });

  it('keeps each item\'s label, state and action', () => {
    let ran = 0;
    const [open, group] = menuItems([
      { label: 'Open', run: () => (ran += 1), enabled: false },
      { label: 'Empty', items: [] },
    ]) as { text: string; enabled: boolean; action?: () => void }[];
    expect(open).toMatchObject({ text: 'Open', enabled: false });
    open.action?.();
    expect(ran).toBe(1);
    expect(group).toMatchObject({ text: 'Empty', enabled: false });
  });
});
