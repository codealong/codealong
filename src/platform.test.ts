import { afterEach, describe, expect, it } from 'vitest';
import { CHORDS, PR_CHORDS, SUGGESTION_CHORDS } from './menus';
import { VIEW_CHORDS } from './mainKeys';
import { capsSaid, chordKeys, chordPressed, chordSigns, cmd, contextPress, ctrl, emacsCtrl, held, keysIn, keysSaid, parseChord, platform, platformChord, setPlatform, writeChord } from './platform';
import { AWAY_CHORD, QUEUE_CHORDS } from './queue';
import { SNOOZE_CHORDS } from './snooze';

afterEach(() => setPlatform(null));

/** A key event: `key` as it types, with `code` and the modifiers held. */
const press = (key: string, mods: { meta?: boolean; ctrl?: boolean; alt?: boolean; shift?: boolean } = {}, code = '') => ({ key, code, metaKey: !!mods.meta, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: !!mods.shift });

/** Every chord the windows have, as they're written for macOS: the tables, and the ones the
 *  handlers write out (mainKeys.ts, Quick.tsx, SessionReply.tsx, PrBox.tsx, zoom.ts…). */
const APP_CHORDS = [
  ...Object.values(CHORDS),
  ...Object.values(PR_CHORDS),
  ...Object.values(QUEUE_CHORDS).flat(),
  ...SUGGESTION_CHORDS,
  ...SNOOZE_CHORDS,
  ...Object.values(VIEW_CHORDS),
  AWAY_CHORD,
  ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((d) => `Alt+Cmd+${d}`),
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((d) => `Cmd+${d}`),
  ...['Ctrl+Cmd+S', 'Ctrl+Cmd+Up', 'Ctrl+Cmd+Down', 'Ctrl+Tab', 'Ctrl+Shift+Tab', 'Ctrl+Enter'],
  ...['Cmd+K', 'Cmd+P', 'Cmd+N', 'Cmd+F', 'Cmd+,', 'Cmd+=', 'Cmd+-', 'Cmd+0', 'Cmd+A', 'Cmd+C', 'Cmd+O', 'Cmd+D', 'Cmd+T', 'Cmd+L', 'Cmd+W', 'Cmd+Q', 'Cmd+H', 'Cmd+S', 'Cmd+.'],
  ...['Cmd+Up', 'Cmd+Down', 'Cmd+Enter', 'Cmd+Backspace'],
  ...['Alt+Cmd+F', 'Alt+Cmd+C', 'Alt+Cmd+Enter', 'Alt+Enter', 'Alt+Up', 'Alt+Down'],
  ...['Shift+Cmd+N', 'Shift+Cmd+F', 'Shift+Cmd+A', 'Shift+Cmd+U', 'Shift+Cmd+Up', 'Shift+Cmd+Down'],
  ...['Shift+Tab', 'Shift+Enter', 'Shift+Up', 'Shift+Down', 'Shift+0', 'Shift+1', 'Shift+2'],
];

describe('on macOS', () => {
  it('is the platform where nothing says otherwise', () => {
    expect(platform()).toBe('mac');
  });

  it('presses and says chords as they are written', () => {
    setPlatform('mac');
    for (const c of APP_CHORDS) expect(platformChord(c)).toBe(c);
    expect(chordSigns('Ctrl+Cmd+W')).toBe('⌃⌘W');
    expect(chordSigns('Alt+Shift+Down')).toBe('⌥⇧↓');
    expect(chordSigns('Ctrl+Enter')).toBe('⌃⏎');
    expect(chordKeys('Shift+Cmd+E')).toEqual(['⇧', '⌘', 'E']);
    expect(chordPressed('Cmd+E', press('e', { meta: true }))).toBe(true);
    expect(chordPressed('Cmd+E', press('e', { ctrl: true }))).toBe(false);
    expect(chordPressed('Ctrl+Cmd+W', press('w', { ctrl: true, meta: true }))).toBe(true);
    expect(chordPressed('Alt+Cmd+3', press('£', { alt: true, meta: true }, 'Digit3'))).toBe(true);
    expect(held(press('k', { meta: true }), 'Cmd')).toBe(true);
    expect(cmd(press('k', { meta: true }))).toBe(true);
    expect(cmd(press('k', { ctrl: true }))).toBe(false);
    expect(emacsCtrl(press('n', { ctrl: true }), 'n')).toBe(true);
    expect(contextPress({ button: 0, ctrlKey: true })).toBe(true);
    // ⌃ with whatever else is held, as ⌃⇧⌘S and a picker's ⌃⌘N have always gone.
    expect(ctrl(press('s', { ctrl: true, meta: true, shift: true }))).toBe(true);
    expect(ctrl(press('s', { alt: true, meta: true }))).toBe(false);
    expect(emacsCtrl(press('n', { ctrl: true, meta: true }), 'n')).toBe(true);
  });

  it('leaves text and keycaps as they are', () => {
    setPlatform('mac');
    for (const t of ['Hide the sidebar (⌃⌘S)', '⌘-click adds it', 'Send now (⌃⏎), into the turn it’s taking', '⏎ renames it']) expect(keysIn(t)).toBe(t);
    expect(keysSaid('⌥⌘3')).toBe('⌥⌘3');
    expect(capsSaid(['⌘', 'K'])).toEqual(['⌘', 'K']);
  });
});

describe('on Linux', () => {
  it('presses ⌘ as Ctrl, ⌥ and ⌃ as Alt, ⇧ as Shift', () => {
    setPlatform('linux');
    expect(platformChord('Cmd+E')).toBe('Ctrl+E');
    expect(platformChord('Shift+Cmd+E')).toBe('Ctrl+Shift+E');
    expect(platformChord('Ctrl+Cmd+W')).toBe('Ctrl+Alt+W');
    expect(platformChord('Alt+Shift+Down')).toBe('Alt+Shift+Down');
    expect(platformChord('Alt+Cmd+Enter')).toBe('Ctrl+Alt+Enter');
    expect(platformChord('Ctrl+-')).toBe('Alt+-');
    expect(platformChord('Cmd+]')).toBe('Ctrl+]');
  });

  it('keeps the few chords the rule would make another’s, or the desktop’s', () => {
    setPlatform('linux');
    expect(platformChord('Alt+Cmd+3')).toBe('Alt+3');
    expect(platformChord('Ctrl+Cmd+3')).toBe('Ctrl+Alt+3');
    expect(platformChord('Ctrl+Tab')).toBe('Ctrl+Tab');
    expect(platformChord('Ctrl+Shift+Tab')).toBe('Ctrl+Shift+Tab');
    expect(platformChord('Ctrl+`')).toBe('Ctrl+`');
    expect(platformChord('Ctrl+Enter')).toBe('Ctrl+Shift+Enter');
  });

  it('gives every chord the windows have keys of its own', () => {
    setPlatform('linux');
    const byKeys = new Map<string, string>();
    for (const c of new Set(APP_CHORDS.map((c) => writeChord(parseChord(c))))) {
      const keys = platformChord(c);
      expect(byKeys.get(keys), `${c} and ${byKeys.get(keys)} are both ${keys}`).toBeUndefined();
      byKeys.set(keys, c);
    }
    // And none is a key the desktop keeps for itself (GNOME's and most others').
    for (const desktop of ['Alt+Tab', 'Alt+Shift+Tab', 'Alt+F4', 'Alt+F2', 'Alt+Space', 'Ctrl+Alt+T', 'Ctrl+Alt+Delete', 'Ctrl+Alt+L', 'Ctrl+Alt+Left', 'Ctrl+Alt+Right']) {
      expect(byKeys.get(desktop), `${byKeys.get(desktop)} is the desktop's ${desktop}`).toBeUndefined();
    }
  });

  it('matches the keys as WebKitGTK reports them, Ctrl for ⌘ and no Super', () => {
    setPlatform('linux');
    expect(chordPressed('Cmd+E', press('e', { ctrl: true }))).toBe(true);
    expect(chordPressed('Cmd+E', press('e', { meta: true }))).toBe(false);
    expect(chordPressed('Ctrl+Cmd+W', press('w', { ctrl: true, alt: true }))).toBe(true);
    expect(chordPressed('Ctrl+Cmd+W', press('w', { ctrl: true }))).toBe(false);
    expect(chordPressed('Alt+Cmd+3', press('3', { alt: true }, 'Digit3'))).toBe(true);
    expect(chordPressed('Ctrl+Cmd+3', press('3', { ctrl: true, alt: true }, 'Digit3'))).toBe(true);
    expect(chordPressed('Ctrl+Enter', press('Enter', { ctrl: true, shift: true }))).toBe(true);
    expect(chordPressed('Ctrl+Enter', press('Enter', { ctrl: true }))).toBe(false);
    expect(chordPressed('Cmd+Enter', press('Enter', { ctrl: true }))).toBe(true);
    expect(chordPressed('Ctrl+-', press('-', { alt: true }, 'Minus'))).toBe(true);
    expect(held(press('k', { ctrl: true }), 'Cmd')).toBe(true);
    expect(held(press('k', { ctrl: true, alt: true }), 'Ctrl', 'Cmd')).toBe(true);
    expect(cmd(press('k', { ctrl: true }))).toBe(true);
    expect(cmd(press('k', { meta: true }))).toBe(false);
    // ⌃ is Alt there.
    expect(ctrl(press('s', { ctrl: true, alt: true }))).toBe(true);
    expect(ctrl(press('s', { ctrl: true }))).toBe(false);
    // ⌃N and ⌃P aren't Linux's, and nor is ⌃-click: Ctrl there is ⌘'s.
    expect(emacsCtrl(press('n', { ctrl: true }), 'n')).toBe(false);
    expect(contextPress({ button: 0, ctrlKey: true })).toBe(false);
  });

  it('says keys in words joined with +, arrows as arrows', () => {
    setPlatform('linux');
    expect(chordSigns('Ctrl+Cmd+W')).toBe('Ctrl+Alt+W');
    expect(chordSigns('Alt+Shift+Down')).toBe('Alt+Shift+↓');
    expect(chordSigns('Cmd+Enter')).toBe('Ctrl+Enter');
    expect(chordSigns('Ctrl+Enter')).toBe('Ctrl+Shift+Enter');
    expect(chordSigns('Esc')).toBe('Esc');
    expect(chordKeys('Shift+Cmd+E')).toEqual(['Ctrl', 'Shift', 'E']);
    expect(keysSaid('⌥⌘3')).toBe('Alt+3');
    expect(keysSaid('⌘O')).toBe('Ctrl+O');
    expect(keysSaid('Ctrl+Alt+W')).toBe('Ctrl+Alt+W');
    expect(capsSaid(['⌘', 'K'])).toEqual(['Ctrl', 'K']);
    expect(capsSaid(['⌃⌘↑'])).toEqual(['Ctrl', 'Alt', '↑']);
    expect(capsSaid(['⏎'])).toEqual(['Enter']);
    // A global shortcut's keys are in Linux's words already.
    expect(capsSaid(['Ctrl', 'Alt', '/'])).toEqual(['Ctrl', 'Alt', '/']);
  });

  it('says the keys a sentence names as Linux names them', () => {
    setPlatform('linux');
    expect(keysIn('Hide the sidebar (⌃⌘S)')).toBe('Hide the sidebar (Ctrl+Alt+S)');
    expect(keysIn('⌘-click adds a todo or leaves it out, ⇧-click adds a run of them.')).toBe('Ctrl-click adds a todo or leaves it out, Shift-click adds a run of them.');
    expect(keysIn('Send now (⌃⏎), into the turn it’s taking')).toBe('Send now (Ctrl+Shift+Enter), into the turn it’s taking');
    expect(keysIn('⌃Tab goes to the next view')).toBe('Ctrl+Tab goes to the next view');
    expect(keysIn('Go to this prompt (⌘⇧↑)')).toBe('Go to this prompt (Ctrl+Shift+↑)');
    expect(keysIn('Settings (⌘,)')).toBe('Settings (Ctrl+,)');
    expect(keysIn('Zoom to fit (⇧1). ⇧2 zooms to the selection, ⇧0 to 100%.')).toBe('Zoom to fit (Shift+1). Shift+2 zooms to the selection, Shift+0 to 100%.');
    expect(keysIn('⏎ renames it · Esc keeps the old name')).toBe('Enter renames it · Esc keeps the old name');
    expect(keysIn('↑ ↓ choose · ⇥ finishes the name · esc puts the list away')).toBe('↑ ↓ choose · Tab finishes the name · esc puts the list away');
    expect(keysIn('⌥⏎ shows the session in Codealong')).toBe('Alt+Enter shows the session in Codealong');
    expect(keysIn('Nothing to press here')).toBe('Nothing to press here');
    expect(keysIn(undefined)).toBeUndefined();
  });
});
