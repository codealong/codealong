// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import type { FolderLook } from './types';
import { useFolderLook } from './useFolderLook';

// A folder's path typed in the quick panel's repo menu (`useFolderLook`): what goes to the engine
// to look at it (`look_folder`), and what the menu has to offer of it meanwhile.

/** What the engine finds at a folder typed. */
const found = (typed: string, over: Partial<FolderLook> = {}): FolderLook => ({ typed, path: typed.replace('~', '/Users/me'), shown: typed, name: typed.split('/').pop() ?? '', exists: true, completions: [], ...over });
const lookedAt = () => engine.sent('look_folder').map((a) => a.typed);

interface Menu {
  picker: string | null;
  deviceNow: string | null;
}

/** The panel with the repo menu open (or another, or none), starting on this machine or another. */
async function menu(picker: string | null = 'repo', deviceNow: string | null = null) {
  const w = mountHook((m: Menu) => useFolderLook(m.picker, m.deviceNow), { picker, deviceNow });
  await settle();
  return {
    get now() {
      return w.current;
    },
    /** What the menu's filter holds, as it's typed. */
    type: async (typed: string) => {
      act(() => w.current.setFolderTyped(typed));
      await settle();
    },
    open: async (which: string | null) => {
      picker = which;
      w.rerender({ picker, deviceNow });
      await settle();
    },
  };
}

describe('a folder’s path typed in the quick panel’s repo menu', () => {
  it('is looked at as it’s typed, and offered once the engine has answered for just what’s typed', async () => {
    engine.answer('look_folder', (args: Record<string, unknown>) => found(String(args.typed)));
    const m = await menu();
    expect([m.now.folderTyped, m.now.typingFolder, m.now.looked, lookedAt()]).toEqual(['', false, null, []]);
    await m.type('~/projects/northwind-app');
    expect([m.now.folderTyped, m.now.typingFolder, lookedAt()]).toEqual(['~/projects/northwind-app', true, ['~/projects/northwind-app']]);
    expect(m.now.looked).toEqual(found('~/projects/northwind-app'));
  });

  it('is a path from your home folder or the top of the disk, and anything else is a name to find a repo by', async () => {
    engine.answer('look_folder', (args: Record<string, unknown>) => found(String(args.typed)));
    const m = await menu();
    for (const typed of ['northwind', 'app/src', './here']) {
      await m.type(typed);
      expect([m.now.folderTyped, m.now.typingFolder, m.now.looked], typed).toEqual([typed, false, null]);
    }
    expect(lookedAt()).toEqual([]);
    for (const typed of ['~', '~/atlas', '/srv/kestrel']) {
      await m.type(typed);
      expect([m.now.typingFolder, m.now.looked?.typed], typed).toEqual([true, typed]);
    }
  });

  it('offers nothing while the answer is still for what was typed before, whichever answer comes first', async () => {
    const answers: Record<string, (l: FolderLook) => void> = {};
    engine.answer('look_folder', (args: Record<string, unknown>) => new Promise<FolderLook>((resolve) => (answers[String(args.typed)] = resolve)));
    const m = await menu();
    await m.type('~/atl');
    await m.type('~/atlas');
    expect([lookedAt(), m.now.looked]).toEqual([['~/atl', '~/atlas'], null]);
    // The earlier look answers late: it isn't what's typed now.
    answers['~/atl'](found('~/atl', { exists: false }));
    await settle();
    expect(m.now.looked).toBeNull();
    answers['~/atlas'](found('~/atlas'));
    await settle();
    expect(m.now.looked).toEqual(found('~/atlas'));
    // Typed on, the last answer isn't for it.
    await m.type('~/atlas/site');
    expect(m.now.looked).toBeNull();
  });

  it('keeps what it found for what’s typed when the look before it answers last', async () => {
    const answers: Record<string, (l: FolderLook) => void> = {};
    engine.answer('look_folder', (args: Record<string, unknown>) => new Promise<FolderLook>((resolve) => (answers[String(args.typed)] = resolve)));
    const m = await menu();
    await m.type('~/atl');
    await m.type('~/atlas');
    answers['~/atlas'](found('~/atlas'));
    await settle();
    expect(m.now.looked).toEqual(found('~/atlas'));
    // The look it replaced comes in after it, with the menu still open: it's for nothing typed
    // now, and takes nothing away.
    answers['~/atl'](found('~/atl', { exists: false }));
    await settle();
    expect([m.now.folderTyped, m.now.looked]).toEqual(['~/atlas', found('~/atlas')]);
  });

  it('says why, as a folder that can’t be added, when the look fails', async () => {
    engine.answer('look_folder', () => {
      throw new Error('No such disk');
    });
    const m = await menu();
    await m.type('/Volumes/kestrel');
    expect(m.now.looked).toEqual({ typed: '/Volumes/kestrel', path: '', shown: '/Volumes/kestrel', name: '', exists: false, refused: 'No such disk', completions: [] });
  });

  it('isn’t looked at for another machine, whose folders aren’t this one’s, nor with another menu open', async () => {
    engine.answer('look_folder', (args: Record<string, unknown>) => found(String(args.typed)));
    const elsewhere = await menu('repo', 'a1b2c3d4e5');
    await elsewhere.type('~/atlas');
    expect([elsewhere.now.folderTyped, elsewhere.now.typingFolder, elsewhere.now.looked]).toEqual(['~/atlas', false, null]);
    const agent = await menu('agent');
    await agent.type('~/atlas');
    expect(agent.now.looked).toBeNull();
    expect(lookedAt()).toEqual([]);
  });

  it('forgets what was typed once the menu is put away, and looks afresh the next time', async () => {
    engine.answer('look_folder', (args: Record<string, unknown>) => found(String(args.typed)));
    const m = await menu();
    await m.type('~/atlas');
    await m.open(null);
    expect([m.now.folderTyped, m.now.typingFolder, m.now.looked]).toEqual(['', false, null]);
    await m.open('repo');
    expect([m.now.folderTyped, m.now.looked]).toEqual(['', null]);
    await m.type('~/atlas');
    expect([m.now.looked?.typed, lookedAt()]).toEqual(['~/atlas', ['~/atlas', '~/atlas']]);
    // Another menu opened over it puts it away too.
    await m.open('worktree');
    expect(m.now.folderTyped).toBe('');
  });

  it('drops an answer that comes after the menu is put away', async () => {
    let answer: (l: FolderLook) => void = () => {};
    engine.answer('look_folder', () => new Promise<FolderLook>((resolve) => (answer = resolve)));
    const m = await menu();
    await m.type('~/atlas');
    await m.open(null);
    answer(found('~/atlas'));
    await settle();
    await m.open('repo');
    await m.type('~/atlas');
    // Asked again, and not offered from the answer that came while it was away.
    expect([lookedAt().length, m.now.looked]).toEqual([2, null]);
  });
});
