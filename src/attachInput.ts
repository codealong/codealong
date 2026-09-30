// Attachments into text: files pasted from the clipboard (a screenshot, or files copied in the
// Finder, which WebKit hands a page as files), dropped from the Finder, or picked. Each is kept
// by the engine (attachments.rs) and goes into the text at the cursor: an image as
// `![image](images/…)`, which shows in place, any other file as `[name](files/…)`.

import { useEffect, useRef, type ClipboardEvent as ReactClipboardEvent, type RefObject } from 'react';
import { importAttachment, onFileDrop, saveAttachment } from './api';
import type { Attachment } from './types';
import { errorText } from './report';

/**
 * The files a paste should attach, or none to paste text as usual. Apps like Numbers put a
 * picture of what you copied next to its text; the text wins there. Files copied in the Finder
 * come with no text (WebKit withholds it), so they're attached.
 */
export function filesToAttach(e: ReactClipboardEvent): File[] {
  const listed = [...e.clipboardData.files];
  const files = listed.length > 0 ? listed : [...e.clipboardData.items].filter((i) => i.kind === 'file').flatMap((i) => i.getAsFile() ?? []);
  if (files.length === 0) return [];
  const text = e.clipboardData.getData('text/plain');
  const onlyPictures = files.every((f) => f.type.startsWith('image/'));
  return text.trim() && onlyPictures ? [] : files;
}

/** Keeps each file, in order, and says how text refers to each. */
export async function keepFiles(files: File[]): Promise<Attachment[]> {
  const kept: Attachment[] = [];
  for (const f of files) kept.push(await saveAttachment(f, f.name || 'file'));
  return kept;
}

/**
 * Calls `onAttachments` with the files dropped from the Finder onto the element `ref` points
 * at, once they're kept. Tauri takes file drops before the page sees them, so this listens for
 * its drop event and checks what's on top where the drop landed: a sheet over a panel takes the
 * drop, not the panel. A file that can't be kept (a folder, one too big) is reported; the rest go
 * in.
 */
export function useFileDrop(ref: RefObject<HTMLElement | null>, onAttachments: (kept: Attachment[]) => void, onError: (message: string) => void, enabled = true) {
  const latest = useRef({ onAttachments, onError });
  latest.current = { onAttachments, onError };
  useEffect(() => {
    if (!enabled) return;
    return onFileDrop((paths, at) => {
      const el = ref.current;
      const hit = document.elementFromPoint(at.x, at.y);
      if (!el || !hit || !el.contains(hit)) return;
      void (async () => {
        const kept: Attachment[] = [];
        for (const p of paths) {
          try {
            kept.push(await importAttachment(p));
          } catch (e: unknown) {
            latest.current.onError(errorText(e));
          }
        }
        if (kept.length > 0) latest.current.onAttachments(kept);
      })();
    });
  }, [ref, enabled]);
}
