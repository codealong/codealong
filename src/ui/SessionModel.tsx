// The model a running session is on, beside Send in its box (SessionReply.tsx): the one its
// latest reply came from, or that it was switched to since (`Session.model`). Where Codealong can
// switch it (one it runs, through its agent; one whose terminal it types into, by its harness's
// command, `StartAccount.switchesModel`), it's a menu of its account's models, and picking one
// switches that session alone (send.rs `switch_model`). Not while its turn runs or it waits on
// you, where the command would go in as a prompt or answer it.

import { agentOptions, modelName, parseAgent } from '../start';
import type { Session, StartAccount } from '../types';
import { ModelMenu } from './ModelMenu';

export function SessionModel({ session: s, account, switches, busy, onSwitch, onDone }: { session: Session; account: StartAccount | undefined; switches: boolean; busy: boolean; onSwitch: (model: string) => void; onDone: () => void }) {
  const models = account?.models ?? [];
  const label = s.model ? modelName(s.model, models) : 'Model';
  if (!switches || !account) {
    return (
      <span className="ca-modelmenu-said" title="The model its latest reply came from">
        {label}
      </span>
    );
  }
  return (
    <ModelMenu
      account={account}
      // Only a model by its name: the account's own isn't one `/model` takes.
      lines={agentOptions([account], models, null).filter((o) => o.model)}
      models={models}
      // Which of the names it's on isn't known for sure (a reply names the model without its
      // context size), so none is checked; the chip says what it's on.
      value={null}
      label={label}
      title={busy ? 'Switch its model once its turn ends' : 'Switch its model, for this session alone'}
      disabled={busy}
      onPick={(id) => {
        const { model } = parseAgent(id);
        if (model) onSwitch(model);
      }}
      onDone={onDone}
    />
  );
}
