import { describe, expect, it } from 'vitest';
import { allEditors, folderTargets, otherEditors, withOtherEditors } from './editors';
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

const t = (id: string, group: Target['group'] = 'lane'): Target => ({ id, icon: 'code', label: id, detail: null, kbd: null, disabled: false, group });

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

  it('opens a session’s folder in any of them', () => {
    expect(folderTargets(apps).map((x) => [x.id, x.label])).toEqual([
      ['l|editor|cursor', 'Open folder in Cursor'],
      ['l|editor|vscode', 'Open folder in VS Code'],
      ['l|editor|zed', 'Open folder in Zed'],
    ]);
  });
});
