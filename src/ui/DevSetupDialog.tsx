// Improve Codealong in the installed app (a release, the board's `improve.installed`): no change
// to Codealong's source reaches the copy that's running, so before the panel asks what to change
// it suggests switching to Codealong Dev, a build of the source, and offers a session that sets
// it up (improve.ts `devSetupPrompt`). Quick.tsx holds it over the panel, so the button and the
// shortcut ask alike; Not now goes on to Improve Codealong as it is.

import { useState } from 'react';
import { Button, CheckBox, Dialog } from './primitives';

interface DevSetupDialogProps {
  /** The account the session starts on, the panel's; null when there's none to start on. */
  account: string | null;
  /** The checkout of Codealong's source the session starts in, as it's shown; null when this
   *  machine has none, and it starts in a chat of its own to clone one. */
  source: string | null;
  /** Set it up, or Not now and the dialog closed; `dontAsk` when it shouldn't be suggested again,
   *  which holds whichever way it's answered. */
  onSetUp: (dontAsk: boolean) => void;
  onClose: (dontAsk: boolean) => void;
}

export function DevSetupDialog({ account, source, onSetUp, onClose }: DevSetupDialogProps) {
  const [dontAsk, setDontAsk] = useState(false);
  return (
    <Dialog
      className="ca-devsetup"
      title="Switch to Codealong Dev?"
      lede="This is the installed Codealong, which changes only with a new release, so what an agent changes in Codealong’s source won’t show up here. Codealong Dev is built from that source on this machine: it keeps your settings and todos, and runs each change once it’s built."
      onClose={() => onClose(dontAsk)}
      onSubmit={() => onSetUp(dontAsk)}
      aside={
        <label className="ca-check">
          <CheckBox checked={dontAsk} onChange={setDontAsk} />
          Don’t suggest this again
        </label>
      }
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={() => onClose(dontAsk)}>
            Not now
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!account} autoFocus>
            {account ? `Set it up with ${account}` : 'No account to start on'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <p className="ca-devsetup-note">
          {source
            ? `An agent can switch you over, in ${source}: it installs what building Codealong needs, then builds Codealong Dev and opens it in place of this one.`
            : 'An agent can switch you over: it gets the source from GitHub, installs what building it needs, then builds Codealong Dev and opens it in place of this one.'}
        </p>
      </div>
    </Dialog>
  );
}
