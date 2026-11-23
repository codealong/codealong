// Shared click and right-click actions for any local file named in a conversation (Markdown.tsx
// `PathCode`, `FileLink`, `FileHref`, LocalImage.tsx, SentFiles.tsx, SessionThread.tsx).

import { useState, type MouseEvent } from 'react';
import { editFile, openFile, revealFile, yourEditor } from '../api';
import { showContextMenu } from '../contextMenu';
import type { PathAt } from '../filePaths';
import { fileMenu } from '../menus';
import { errorText } from '../report';
import type { PathKind } from '../types';

/** What a file a conversation names does: a click opens it, in your editor at its line where the
 *  path gave one, else in the app that opens its kind (launch.rs `open_file`, which shows in the
 *  Finder what opening would run); a right-click offers each way, an optional in-app `preview`
 *  when given, and its path (menus.ts `fileMenu`). What goes wrong is said in place (`why`): a
 *  file in a scratch folder is often gone by the time you look. */
export function useFileActions({ path, line }: PathAt, kind: PathKind, preview?: () => void) {
  const [why, setWhy] = useState<string | null>(null);
  const tried = (work: Promise<void>) => {
    setWhy(null);
    work.catch((err: unknown) => setWhy(errorText(err)));
  };
  const open = () => tried(line != null && kind === 'file' ? editFile(path, line) : openFile(path));
  const menu = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // A copied path needs no word; one that couldn't be copied says so with the rest.
    const report = { error: setWhy, info: () => {} };
    tried(
      yourEditor().then((editor) =>
        showContextMenu(fileMenu(path, kind, { report, editor, preview, open: () => tried(openFile(path)), edit: () => tried(editFile(path, line)), reveal: () => tried(revealFile(path)) })),
      ),
    );
  };
  return { why, open, menu };
}

/** Why a file didn't open, beside it. */
export function FileWhy({ why }: { why: string | null }) {
  return why ? (
    <span className="ca-md-file-why" role="status">
      {why}
    </span>
  ) : null;
}
