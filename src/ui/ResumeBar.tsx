// Where a session that isn't running goes on, and on what, picked in its box (SessionReply.tsx)
// under what you write: the places its account's sessions go on in, as the quick panel's start
// row offers places, and the account's models and effort, as the start row's agent menu offers
// them. What the picks come to is start.ts's `resumeChoice`, a start's rules for that account; the
// box's placeholder says what sending does then, and the send asks for it.

import { useRef, useState, type CSSProperties } from 'react';
import { agentId, agentOptions, effortStep, parseAgent, placeTakesModel, resumeAccount, type ChosenStart, type ResumePicks } from '../start';
import type { StartAccount, StartPlace } from '../types';
import { Picker } from './Picker';
import { Icon } from './primitives';
import { agentLine, EffortFoot, PlaceToggle } from './StartControls';

/** The model menu is never shorter than this, and keeps this far from the window's top. */
const MENU_MIN = 160;
const MENU_MARGIN = 12;

/** The row, for `account`'s session, with what its picks came to (`chosen`). */
export function ResumeBar({ account, picks, chosen, onPicks, onDone }: { account: StartAccount; picks: ResumePicks; chosen: ChosenStart; onPicks: (picks: ResumePicks) => void; onDone: () => void }) {
  const [menu, setMenu] = useState(false);
  // The room above the box, as the menu opens: a window sized to what it shows (the peek) has no
  // more than its conversation's height to open the menu into.
  const [room, setRoom] = useState<number | null>(null);
  const row = useRef<HTMLDivElement>(null);
  const open = () => {
    const top = row.current?.closest('.ca-composer-field')?.getBoundingClientRect().top;
    setRoom(top == null ? null : Math.max(MENU_MIN, top - MENU_MARGIN));
    setMenu(true);
  };
  const a = resumeAccount(account);
  const models = a.models ?? [];
  const efforts = a.efforts ?? [];

  /** Picks where it goes on. The apps choose their model: a model asked for gives way to the
   *  account's own there, as it does for a start (useStartChoice.ts). */
  const pickPlace = (place: StartPlace) => onPicks({ ...picks, place, agent: placeTakesModel(place) ? picks.agent : null });
  /** Picks a model: it's asked for on the command line or of Codealong's adapter, so a place that
   *  takes none gives way to a terminal. */
  const pickModel = (id: string) => {
    const { model } = parseAgent(id);
    const place = model && chosen.placeNow && !placeTakesModel(chosen.placeNow) ? 'terminal' : picks.place;
    onPicks({ ...picks, agent: id, place });
    close();
  };
  const close = () => {
    setMenu(false);
    onDone();
  };

  const section = { key: a.name, options: agentOptions([a], models, null).map((o) => agentLine(o, a, models, chosen.agentNow ?? agentId(a.name, null))) };
  const foot = efforts.length > 0 ? <EffortFoot efforts={efforts} value={picks.effort} onPick={(effort) => onPicks({ ...picks, effort })} harness={a.harness} model={chosen.effortModel} /> : null;

  return (
    <div ref={row} className="ca-resume" style={room == null ? undefined : ({ '--ca-menu-room': `${room}px` } as CSSProperties)}>
      <PlaceToggle label="Goes on in" places={chosen.places} value={chosen.placeNow} onPick={pickPlace} />
      {chosen.takesAModel && (
        <div className="ca-resume-pick">
          <button type="button" data-picker-trigger className="ca-resume-model" title="Model and effort it goes on with" aria-haspopup="dialog" aria-expanded={menu} onClick={() => (menu ? close() : open())}>
            <span>{chosen.modelShown}</span>
            {chosen.effortWord && <span className="ca-resume-model-detail">{chosen.effortWord}</span>}
            <Icon name="chevron-down" size={12} />
          </button>
          {menu && <Picker className="is-up" sections={[section]} value={chosen.agentNow} placeholder="Model" onPick={pickModel} onClose={close} foot={foot} onStep={foot ? (delta) => onPicks({ ...picks, effort: effortStep(efforts, picks.effort, delta) }) : undefined} />}
        </div>
      )}
    </div>
  );
}

/** Whether there's anything to pick: more than one place, or a model. */
export function offersResume(chosen: ChosenStart | null): chosen is ChosenStart {
  return !!chosen && (chosen.places.length > 1 || chosen.takesAModel);
}
