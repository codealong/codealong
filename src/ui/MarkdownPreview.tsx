// In-app preview dialog for a local Markdown file (Antigravity user-facing artifacts, sent `.md`
// files, and `[name](file:///….md)` links): renders the document with Codealong's Markdown view
// (including local images and carousels) while respecting macOS folder privacy (`read_markdown`)
// and keeping one-click actions to open in your editor, default app, Finder, or copy its path.

import { useEffect, useState, type ReactNode } from 'react';
import { editFile, openFile, readMarkdown, revealFile, yourEditor } from '../api';
import { tildePath } from '../format';
import { errorText } from '../report';
import type { Profile } from '../types';
import { CopyButton } from './CopyButton';
import { ImagesFrom } from './DescriptionEditor';
import { Button, Dialog, Icon } from './primitives';

type Load =
  | { kind: 'loading' }
  | { kind: 'asksFirst' }
  | { kind: 'text'; text: string }
  | { kind: 'error'; why: string };

interface MarkdownPreviewProps {
  path: string;
  onClose: () => void;
  render: (text: string) => ReactNode;
}

export function MarkdownPreviewDialog({ path, onClose, render }: MarkdownPreviewProps) {
  const [asked, setAsked] = useState(false);
  const [load, setLoad] = useState<Load>({ kind: 'loading' });
  const [editor, setEditor] = useState<Profile>({ id: 'cursor', name: 'Cursor' });
  const [actError, setActError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    yourEditor().then(
      (ed) => {
        if (live) setEditor(ed);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    setLoad({ kind: 'loading' });
    readMarkdown(path, asked).then(
      (res) => {
        if (!live) return;
        if (res.kind === 'asksFirst') setLoad({ kind: 'asksFirst' });
        else setLoad({ kind: 'text', text: res.text });
      },
      (err: unknown) => {
        if (live) setLoad({ kind: 'error', why: errorText(err) });
      },
    );
    return () => {
      live = false;
    };
  }, [path, asked]);

  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  const name = path.slice(cut + 1) || path;
  const folder = cut > 0 ? tildePath(path.slice(0, cut)) : '';

  const run = (work: Promise<void>) => {
    setActError(null);
    work.catch((err: unknown) => setActError(errorText(err)));
  };

  return (
    <Dialog
      className="ca-mdpreview"
      title={name}
      lede={folder || undefined}
      lifted
      onClose={onClose}
      aside={
        <div className="ca-mdpreview-tools">
          <Button variant="ghost" size="sm" icon="code" onClick={() => run(editFile(path, null))}>
            Open in {editor.name}
          </Button>
          <Button variant="ghost" size="sm" icon="external" onClick={() => run(openFile(path))}>
            Open
          </Button>
          <Button variant="ghost" size="sm" icon="folder" onClick={() => run(revealFile(path))}>
            Reveal in Finder
          </Button>
          <CopyButton text={path} label="Copy path" title="Copy path" />
        </div>
      }
      foot={
        <Button variant="secondary" size="sm" onClick={onClose}>
          Close
        </Button>
      }
    >
      <div className="ca-dialog-body ca-mdpreview-body">
        {actError && (
          <p className="ca-mdpreview-err" role="alert">
            {actError}
          </p>
        )}
        {load.kind === 'loading' && <p className="ca-mdpreview-note">Reading {name}…</p>}
        {load.kind === 'asksFirst' && (
          <div className="ca-mdpreview-ask">
            <Icon name="file-text" size={18} />
            <p>macOS may ask you to let Codealong read this folder.</p>
            <Button variant="secondary" size="sm" icon="eye" onClick={() => setAsked(true)}>
              Show file
            </Button>
          </div>
        )}
        {load.kind === 'error' && (
          <p className="ca-mdpreview-err" role="alert">
            {load.why}
          </p>
        )}
        {load.kind === 'text' && <ImagesFrom.Provider value={null}>{render(load.text)}</ImagesFrom.Provider>}
      </div>
    </Dialog>
  );
}
