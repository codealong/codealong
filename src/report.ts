// How a window says how something went: the main window with a toast, the bar with a note on
// the pill it came from, the quick panel with a line under its input, a desktop card with a note
// on the card. Shared code (menus.ts, open.ts) takes a window's `Report` rather than knowing
// which window it's in.

export interface Report {
  /** What went wrong. */
  error: (message: string) => void;
  /** What happened, when it's worth saying. */
  info: (message: string) => void;
  /** Shows that `work` is under way while it takes a moment (the main window's spinner). */
  busy?: <T>(label: string, work: Promise<T>) => Promise<T>;
}

/** An error as the sentence to show: commands reject with the engine's words, and an `Error`
 *  prints with "Error: " in front. */
export function errorText(e: unknown): string {
  return String(e).replace(/^Error: /, '');
}
