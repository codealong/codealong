// What the main window's usage bar, its notices and Settings ask for an account (App.tsx):
// opening config.toml, signing an account in, turning training off for one, and using one of its
// limit resets. Each says how it went in a toast: the card it was asked from has gone by the time
// it's done.

import { openConfig, spendReset, stopTraining } from './api';
import * as menus from './menus';
import { errorText } from './report';
import { afterStop } from './training';

/** `track` shows that work is under way while it takes a moment. */
export function accountActsOf({ setToast, track }: { setToast: (message: string) => void; track: <T>(label: string, work: Promise<T>) => Promise<T> }) {
  const editConfig = () => {
    openConfig().catch((e: unknown) => setToast(String(e)));
  };
  const startSignIn = (account: string) => {
    menus.signInTo(account).then(setToast, (e: unknown) => setToast(String(e)));
  };
  // The card it was asked from has gone by the time it's done, so how it went is a toast.
  const turnOffTraining = (account: string) => {
    track(`Turning off training for ${account}…`, stopTraining(account)).then(
      (after) => setToast(afterStop(after) ?? `Training is off for ${account}.`),
      (e: unknown) => setToast(errorText(e)),
    );
  };
  const resetLimits = (account: string, credit?: string) => {
    track(`Resetting ${account}’s limits…`, spendReset(account, credit)).then(
      (used) => setToast(used.message),
      (e: unknown) => setToast(errorText(e)),
    );
  };
  return { editConfig, startSignIn, turnOffTraining, resetLimits };
}
