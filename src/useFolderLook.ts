// A folder's path typed in the quick panel's repo menu: what's typed there, and what the engine
// found at it (newfolder.rs), for the menu to offer the folder as a place to start.

import { useEffect, useState } from 'react';
import { lookFolder } from './api';
import { isFolderPath } from './quick';
import { errorText } from './report';
import type { FolderLook } from './types';

/** What's typed in the repo menu's filter (`setFolderTyped` is its `onFilter`) while `picker` is
 *  `'repo'`, and when it's a folder's path on this machine (`typingFolder`; `deviceNow` is the
 *  other machine a session would start on, whose folders aren't this one's to look in), what the
 *  engine found there: `looked`, once the answer for just what's typed is in. Put away, the menu
 *  forgets what was typed. */
export function useFolderLook(picker: string | null, deviceNow: string | null): { folderTyped: string; setFolderTyped: (typed: string) => void; typingFolder: boolean; looked: FolderLook | null } {
  // A folder's path typed in the worktree menu, and what the engine found there.
  const [folderTyped, setFolderTyped] = useState('');
  const [folderLook, setFolderLook] = useState<FolderLook | null>(null);
  // A folder's path typed in the repo menu is looked up as it's typed; an answer goes only
  // with what was typed for it. Another Mac's folders aren't this one's to look in.
  const typingFolder = !deviceNow && isFolderPath(folderTyped);
  useEffect(() => {
    if (picker !== 'repo' || !typingFolder) return;
    let live = true;
    lookFolder(folderTyped)
      .then((l) => live && setFolderLook(l))
      // A look that fails says why in the folder's row.
      .catch((e: unknown) => live && setFolderLook({ typed: folderTyped, path: '', shown: folderTyped, name: '', exists: false, refused: errorText(e), completions: [] }));
    return () => {
      live = false;
    };
  }, [picker, typingFolder, folderTyped]);
  useEffect(() => {
    if (picker !== 'repo') setFolderTyped('');
  }, [picker]);
  const looked = typingFolder && folderLook?.typed === folderTyped ? folderLook : null;
  return { folderTyped, setFolderTyped, typingFolder, looked };
}
