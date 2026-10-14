// Where a session that isn't running goes on, and on what, picked in its box (SessionReply.tsx):
// beside Send, the place its account's sessions go on in, as the quick panel's start row offers
// places, then the account's models and effort, where a running session's box says its model
// (SessionModel.tsx), as the start row's agent menu offers them. What the picks come to is
// start.ts's `resumeChoice`, a start's rules for that account; the box's placeholder says what
// sending does then, and the send asks for it.

import { agentId, agentOptions, effortStep, parseAgent, placeTakesModel, resumeAccount, type ChosenStart, type ResumePicks } from '../start';
import type { StartAccount, StartPlace } from '../types';
import { BoxMenu, ModelMenu } from './ModelMenu';
import { EffortFoot, placeIcon, placeSections } from './StartControls';

interface ResumeProps {
  account: StartAccount;
  picks: ResumePicks;
  /** What the picks came to. */
  chosen: ChosenStart;
  onPicks: (picks: ResumePicks) => void;
}

/** The place it goes on in, as a chip beside the model's, when there's more than one. */
export function ResumePlace({ picks, chosen, onPicks, onDone }: Omit<ResumeProps, 'account'> & { onDone: () => void }) {
  /** Picks where it goes on. The apps choose their model: a model asked for gives way to the
   *  account's own there, as it does for a start (useStartChoice.ts). */
  const pickPlace = (place: StartPlace) => onPicks({ ...picks, place, agent: placeTakesModel(place) ? picks.agent : null });
  const now = chosen.places.find((p) => p.place === chosen.placeNow);
  return (
    <BoxMenu
      kind="place"
      sections={placeSections(chosen.places)}
      value={chosen.placeNow}
      icon={now ? placeIcon(now.place) : undefined}
      label={now?.label ?? 'Goes on in'}
      title="Where it goes on"
      placeholder="Goes on in"
      onPick={(id) => pickPlace(id as StartPlace)}
      onDone={onDone}
    />
  );
}

/** The model and effort it goes on with, as a chip beside Send. */
export function ResumeModel({ account, picks, chosen, onPicks, onDone }: ResumeProps & { onDone: () => void }) {
  const a = resumeAccount(account);
  const models = a.models ?? [];
  const efforts = a.efforts ?? [];
  /** Picks a model: it's asked for on the command line or of Codealong's adapter, so a place that
   *  takes none gives way to a terminal. */
  const pickModel = (id: string) => {
    const { model } = parseAgent(id);
    const place = model && chosen.placeNow && !placeTakesModel(chosen.placeNow) ? 'terminal' : picks.place;
    onPicks({ ...picks, agent: id, place });
  };
  const foot = efforts.length > 0 ? <EffortFoot efforts={efforts} value={picks.effort} onPick={(effort) => onPicks({ ...picks, effort })} harness={a.harness} model={chosen.effortModel} /> : null;
  return (
    <ModelMenu
      account={a}
      lines={agentOptions([a], models, null)}
      models={models}
      value={chosen.agentNow ?? agentId(a.name, null)}
      label={chosen.modelShown}
      // The level as `--effort` takes it (`xhigh`), as a running session's chip says its own.
      detail={chosen.effortNow}
      title="Model and effort it goes on with"
      foot={foot}
      onStep={foot ? (delta) => onPicks({ ...picks, effort: effortStep(efforts, picks.effort, delta) }) : undefined}
      onPick={pickModel}
      onDone={onDone}
    />
  );
}

/** Whether there's anything to pick: more than one place, or a model. */
export function offersResume(chosen: ChosenStart | null): chosen is ChosenStart {
  return !!chosen && (chosen.places.length > 1 || chosen.takesAModel);
}
