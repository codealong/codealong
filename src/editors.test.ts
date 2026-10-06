import { describe, expect, it } from 'vitest';
import { allEditors, folderMenu, folderTargets, openInMenu, otherEditors, withOtherEditors, type AppIcons } from './editors';
import type { MenuAction, MenuGroup } from './contextMenu';
import * as fx from './testing/fixtures';
import type { Target } from './types';

const apps = {
  editor: 'Cursor',
  editorId: 'cursor',
  editors: [
    { id: 'vscode', name: 'VS Code' },
    { id: 'cursor', name: 'Cursor' },
    { id: 'zed', name: 'Zed' },
  ],
};

const t = (id: string, group: Target['group'] = 'lane'): Target => fx.target({ id, icon: 'code', group });

describe('editors', () => {
  it('lists yours first, then the others in menu order', () => {
    expect(otherEditors(apps).map((e) => e.id)).toEqual(['vscode', 'zed']);
    expect(allEditors(apps).map((e) => e.id)).toEqual(['cursor', 'vscode', 'zed']);
    // Yours is listed even when it isn't found installed; a board from before editors has none.
    expect(allEditors({ editor: 'TextMate', editorId: 'TextMate' }).map((e) => e.name)).toEqual(['TextMate']);
  });

  it('puts the other editors after Open in <yours>', () => {
    const lane = [t('s|k|focus', 'session'), t('l|terminal'), t('l|editor'), t('l|finder')];
    const out = withOtherEditors(lane, apps);
    expect(out.map((x) => x.id)).toEqual(['s|k|focus', 'l|terminal', 'l|editor', 'l|editor|vscode', 'l|editor|zed', 'l|finder']);
    expect(out[3].label).toBe('Open in VS Code');
    // Nothing to add without other editors, or where the folder can't open in one.
    expect(withOtherEditors(lane, { ...apps, editors: [] })).toBe(lane);
    expect(withOtherEditors([t('l|terminal')], apps).map((x) => x.id)).toEqual(['l|terminal']);
  });

  it('shows each app’s icon once there are icons', () => {
    const icons: AppIcons = { cursor: { url: 'data:cursor', menu: { rgba: [], width: 36, height: 36 } }, zed: { url: 'data:zed', menu: { rgba: [1], width: 36, height: 36 } } };
    const out = withOtherEditors([t('l|terminal'), t('l|editor')], apps, icons);
    expect(out.map((x) => x.image)).toEqual([undefined, 'data:cursor', undefined, 'data:zed']);
    expect(folderTargets(apps, icons).map((x) => x.image)).toEqual(['data:cursor', undefined, 'data:zed']);
  });

  it('builds the menus: Open in for a worktree, Open folder in for a session', () => {
    const opened: string[] = [];
    const [group] = openInMenu(apps, {}, (id) => opened.push(id)) as MenuGroup[];
    expect(group.label).toBe('Open in');
    expect((group.items as MenuAction[]).map((i) => i.label)).toEqual(['VS Code', 'Zed']);
    (group.items[1] as MenuAction).run();
    expect(opened).toEqual(['l|editor|zed']);
    expect(openInMenu({ ...apps, editors: [] }, {}, () => {})).toEqual([]);

    const [folder] = folderMenu(apps, {}, () => {}) as MenuGroup[];
    expect((folder.items as MenuAction[]).map((i) => i.label)).toEqual(['Cursor', 'VS Code', 'Zed']);
    // With yours the only one, no submenu.
    const one = { editor: 'Cursor', editorId: 'cursor', editors: [{ id: 'cursor', name: 'Cursor' }] };
    const [only] = folderMenu(one, {}, (id) => opened.push(id)) as MenuAction[];
    expect(only.label).toBe('Open folder in Cursor');
    only.run();
    expect(opened.pop()).toBe('l|editor');

    // A session's folder opens in your terminal too, ahead of the editors.
    opened.length = 0;
    const terminal = { name: 'Ghostty', key: 'claude:c:1' };
    const [both] = folderMenu(one, {}, (id) => opened.push(id), terminal) as MenuGroup[];
    expect((both.items as MenuAction[]).map((i) => i.label)).toEqual(['Ghostty', 'Cursor']);
    (both.items as MenuAction[]).forEach((i) => i.run());
    expect(opened.splice(0)).toEqual(['s|claude:c:1|terminal', 'l|editor']);
    const [many] = folderMenu(apps, {}, (id) => opened.push(id), terminal) as MenuGroup[];
    expect((many.items as MenuAction[]).map((i) => i.label)).toEqual(['Ghostty', 'Cursor', 'VS Code', 'Zed']);
    // With no editor here, the terminal alone.
    const [alone] = folderMenu({ editor: '', editorId: '', editors: [] }, {}, (id) => opened.push(id), terminal) as MenuAction[];
    expect(alone.label).toBe('Open folder in Ghostty');
    alone.run();
    expect(opened.pop()).toBe('s|claude:c:1|terminal');
  });

  it('opens a session’s folder in any of them', () => {
    expect(folderTargets(apps).map((x) => [x.id, x.label])).toEqual([
      ['l|editor|cursor', 'Open folder in Cursor'],
      ['l|editor|vscode', 'Open folder in VS Code'],
      ['l|editor|zed', 'Open folder in Zed'],
    ]);
    // And in your terminal first, which says the folder: the session's own, inside its worktree.
    const withTerminal = folderTargets(apps, {}, { name: 'Ghostty', key: 'claude:c:1', folder: '~/app/src-tauri' });
    expect(withTerminal.map((x) => x.id)).toEqual(['s|claude:c:1|terminal', 'l|editor|cursor', 'l|editor|vscode', 'l|editor|zed']);
    expect(withTerminal[0]).toMatchObject({ label: 'Open folder in Ghostty', detail: '~/app/src-tauri', icon: 'terminal', group: 'lane', kbd: null });
    // With its key where the window has one.
    expect(folderTargets(apps, {}, { name: 'Ghostty', key: 'claude:c:1', folder: '~/app', kbd: ['⌃', '`'] })[0].kbd).toEqual(['⌃', '`']);
  });
});
