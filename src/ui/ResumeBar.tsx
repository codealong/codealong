// Where a session that isn't running goes on, and on what, picked in its box (SessionReply.tsx):
// the places its account's sessions go on in, in a row under what you write, as the quick panel's
// start row offers places, and the account's models and effort beside Send, where a running
// session's box says its model (SessionModel.tsx), as the start row's agent menu offers them. What
// the picks come to is start.ts's `resumeChoice`, a start's rules for that account; the box's
// placeholder says what sending does then, and the send asks for it.

import { agentId, agentOptions, effortStep, parseAgent, placeTakesModel, resumeAccount, type ChosenStart, type ResumePicks } from '../start';
import type { StartAccount, StartPlace } from '../types';
import { ModelMenu } from './ModelMenu';
import { EffortFoot, PlaceToggle } from './StartControls';

interface ResumeProps {
  account: StartAccount;
  picks: ResumePicks;
  /** What the picks came to. */
  chosen: ChosenStart;
  onPicks: (picks: ResumePicks) => void;
}

/** The places, as a row under the text. */
export function ResumePlaces({ picks, chosen, onPicks }: Omit<ResumeProps, 'account'>) {
  /** Picks where it goes on. The apps choose their model: a model asked for gives way to the
   *  account's own there, as it does for a start (useStartChoice.ts). */
  const pickPlace = (place: StartPlace) => onPicks({ ...picks, place, agent: placeTakesModel(place) ? picks.agent : null });
  return (
    <div className="ca-composer-tools ca-resume">
      <PlaceToggle label="Goes on in" places={chosen.places} value={chosen.placeNow} onPick={pickPlace} />
    </div>
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
      detail={chosen.effortWord}
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
