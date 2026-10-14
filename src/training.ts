// Whether each account's provider may train its models on what the account sends, as the engine
// reads it (training.rs): which accounts the usage bar flags, and the words its card and
// Settings › Accounts use.
import type { Training } from './types';

/** An account's reading, when it's been read. */
export function trainingOf(list: Training[] | undefined, account: string): Training | undefined {
  return list?.find((t) => t.account === account);
}

/** The usage bar flags it: the provider may train on it, or nothing's chosen yet. An account
 *  Codealong can't read isn't flagged; Settings says why. */
export function flagged(t: Training | undefined): t is Training {
  return !!t && (t.state === 'on' || t.state === 'unset');
}

/** The flag's own words, beside the account's chip. */
export function flagLabel(t: Training): string {
  return t.state === 'unset' ? 'Training not set' : 'Training on';
}

/** The flag's card's heading. */
export function cardTitle(t: Training): string {
  return t.state === 'unset' ? `Training isn't turned off for ${t.account}` : `Training is on for ${t.account}`;
}

/** What Settings › Accounts says of it, as a tag; nothing for an account it can't tell. */
export function trainingTag(t: Training | undefined): { word: string; ok: boolean } | null {
  if (!t || t.state === 'unknown') return null;
  return t.state === 'off' ? { word: 'Not trained on', ok: true } : { word: flagLabel(t), ok: false };
}

/** How a turn-off went, from the reading after it: null when it's off, else why not. */
export function afterStop(t: Training): string | null {
  switch (t.state) {
    case 'off':
      return null;
    case 'unknown':
      return `It was asked to turn off, but Codealong couldn't read it back to check: ${t.note}`;
    default:
      return `It was asked to turn off, but still reads as on. ${t.note}`;
  }
}
