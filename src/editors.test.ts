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
    const [only] = folderMenu({ editor: 'Cursor', editorId: 'cursor', editors: [{ id: 'cursor', name: 'Cursor' }] }, {}, (id) => opened.push(id)) as MenuAction[];
    expect(only.label).toBe('Open folder in Cursor');
    only.run();
    expect(opened.pop()).toBe('l|editor');
  });

  it('opens a session’s folder in any of them', () => {
    expect(folderTargets(apps).map((x) => [x.id, x.label])).toEqual([
      ['l|editor|cursor', 'Open folder in Cursor'],
      ['l|editor|vscode', 'Open folder in VS Code'],
      ['l|editor|zed', 'Open folder in Zed'],
    ]);
  });
});
