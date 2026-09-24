// Images into text: pasted from the clipboard, dropped from the Finder, or picked from a file.
// Each is kept by the engine (images.rs) and goes into the text as `![image](images/<name>)`.

import { useEffect, useRef, type ClipboardEvent as ReactClipboardEvent, type RefObject } from 'react';
import { importImage, onFileDrop, saveImage } from './api';

const IMAGE_FILE = /\.(png|jpe?g|gif|webp)$/i;

/** The image files on the clipboard of a paste. */
export function pastedImages(e: ReactClipboardEvent): File[] {
  return [...e.clipboardData.items].filter((i) => i.kind === 'file' && i.type.startsWith('image/')).flatMap((i) => i.getAsFile() ?? []);
}

/** Keeps each image, in order, and returns their names. */
export async function keepImages(images: Blob[]): Promise<string[]> {
  const names: string[] = [];
  for (const image of images) names.push(await saveImage(image));
  return names;
}

/**
 * Calls `onNames` with the images dropped from the Finder onto the element `ref` points at,
 * once they're kept. Tauri takes file drops before the page sees them, so this listens for its
 * drop event and checks where the drop landed.
 */
export function useImageDrop(ref: RefObject<HTMLElement | null>, onNames: (names: string[]) => void, onError: (message: string) => void, enabled = true) {
  const latest = useRef({ onNames, onError });
  latest.current = { onNames, onError };
  useEffect(() => {
    if (!enabled) return;
    return onFileDrop((paths, at) => {
      const el = ref.current;
      // The element on top where it landed: a sheet over a panel takes the drop, not the panel.
      const hit = document.elementFromPoint(at.x, at.y);
      if (!el || !hit || !el.contains(hit)) return;
      const images = paths.filter((p) => IMAGE_FILE.test(p));
      if (images.length === 0) {
        latest.current.onError('Only PNG, JPEG, GIF and WebP images can go in.');
        return;
      }
      Promise.all(images.map(importImage))
        .then((names) => latest.current.onNames(names))
        .catch((e: unknown) => latest.current.onError(String(e).replace(/^Error: /, '')));
    });
  }, [ref, enabled]);
}
