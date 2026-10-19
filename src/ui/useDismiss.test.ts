import { describe, expect, it, vi } from 'vitest';
import { escapeTopmost, keyLeavesHover, pressedOutside, typedOn } from './useDismiss';

/** A stand-in for an element: `inside` names what it contains, `matches` what its ancestors match. */
const node = (name: string, matches: string[] = []) => ({ name, closest: (sel: string) => (matches.includes(sel) ? {} : null) }) as unknown as Element;
const box = (...names: string[]) => ({ contains: (t: Node | null) => names.includes((t as unknown as { name: string }).name) });

describe('pressedOutside', () => {
  it('is a press on anything the popover does not contain', () => {
    expect(pressedOutside(node('item'), box('item'))).toBe(false);
    expect(pressedOutside(node('row'), box('item'))).toBe(true);
  });

  it('is any press at all when there is nothing to be inside of', () => {
    expect(pressedOutside(node('item'), null)).toBe(true);
    expect(pressedOutside(null, box('item'))).toBe(true);
  });

  it('lets the button that opened it, or a part drawn elsewhere, count as inside', () => {
    const trigger = node('button', ['[data-picker-trigger]']);
    expect(pressedOutside(trigger, box('item'))).toBe(true);
    expect(pressedOutside(trigger, box('item'), '[data-picker-trigger]')).toBe(false);
    expect(pressedOutside(node('row'), box('item'), '[data-picker-trigger]')).toBe(true);
    const anchor = box('button');
    expect(pressedOutside(trigger, box('item'), (t) => anchor.contains(t))).toBe(false);
    expect(pressedOutside(node('row'), box('item'), (t) => anchor.contains(t))).toBe(true);
  });
});

describe('escapeTopmost', () => {
  const key = (over: object = {}) => ({ key: 'Escape', defaultPrevented: false, isComposing: false, preventDefault: vi.fn(), stopPropagation: vi.fn(), ...over });

  it('closes the one opened last and takes the key', () => {
    const dialog = { close: vi.fn() };
    const picker = { close: vi.fn() };
    const e = key();
    expect(escapeTopmost(e, [dialog, picker])).toBe(true);
    expect(picker.close).toHaveBeenCalledTimes(1);
    expect(dialog.close).not.toHaveBeenCalled();
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).toHaveBeenCalled();
    // With the picker gone, the next one is the dialog's.
    expect(escapeTopmost(key(), [dialog])).toBe(true);
    expect(dialog.close).toHaveBeenCalledTimes(1);
  });

  it('leaves a key that is not Escape, one a field took, and one that ends a composition', () => {
    const open = { close: vi.fn() };
    expect(escapeTopmost(key({ key: 'Enter' }), [open])).toBe(false);
    expect(escapeTopmost(key({ defaultPrevented: true }), [open])).toBe(false);
    expect(escapeTopmost(key({ isComposing: true }), [open])).toBe(false);
    expect(open.close).not.toHaveBeenCalled();
  });

  it('does nothing, and takes nothing, with nothing up', () => {
    const e = key();
    expect(escapeTopmost(e, [])).toBe(false);
    expect(e.preventDefault).not.toHaveBeenCalled();
  });
});

describe('keyLeavesHover', () => {
  const press = (key: string, mods: { meta?: boolean; ctrl?: boolean; alt?: boolean } = {}) => ({ key, metaKey: !!mods.meta, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: false });

  it('keeps a hover card up for a modifier on its own and for ⌘C', () => {
    for (const k of ['Meta', 'Shift', 'Alt', 'Control', 'CapsLock']) expect(keyLeavesHover(press(k))).toBe(true);
    expect(keyLeavesHover(press('c', { meta: true }))).toBe(true);
    expect(keyLeavesHover(press('C', { meta: true }))).toBe(true);
  });

  it('puts it away for any other key, chords with C included', () => {
    expect(keyLeavesHover(press('a'))).toBe(false);
    expect(keyLeavesHover(press('c'))).toBe(false);
    expect(keyLeavesHover(press('Enter'))).toBe(false);
    expect(keyLeavesHover(press('v', { meta: true }))).toBe(false);
    expect(keyLeavesHover(press('c', { meta: true, ctrl: true }))).toBe(false);
    expect(keyLeavesHover(press('c', { meta: true, alt: true }))).toBe(false);
  });
});

describe('typedOn', () => {
  /** A stand-in for what a key went to: the selectors it matches, and whether it's editable. */
  const target = (name: string, matches: string, editable = false) => ({ name, isContentEditable: editable, matches: (sel: string) => sel === matches }) as unknown as HTMLElement;
  const FIELDS = 'textarea, select, input:not([type="checkbox"]):not([type="radio"])';

  it('keeps a hover card up for a key typed into a field on it', () => {
    expect(typedOn(target('title', FIELDS), box('title'))).toBe(true);
    expect(typedOn(target('note', '', true), box('note'))).toBe(true);
  });

  it('is not a key typed anywhere else, nor on a button or a box to tick on it', () => {
    expect(typedOn(target('reply', FIELDS), box('title'))).toBe(false);
    expect(typedOn(target('merge', 'button'), box('merge'))).toBe(false);
    expect(typedOn(null, box('title'))).toBe(false);
    expect(typedOn(target('title', FIELDS), null)).toBe(false);
  });
});
