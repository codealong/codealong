// A session's account, model and effort, as one chip at the right of its box (SessionReply.tsx),
// beside Send: the agent menu's account lines and the rows under them (AgentMenu.tsx `AgentFoot`)
// in a box menu (ModelMenu.tsx `BoxMenu`), as the quick panel starts one with (asked 2026-10-19).
//
// What its menu has goes by what can be done with the session:
// - Running, its account's models where Codealong can switch them (one it runs, through its agent;
//   one whose terminal it types into, by its harness's commands, `StartAccount.switchesModel` and
//   `switchesEffort`; one whose app goes on with it, through the app, `switchesInApp`), and the
//   levels under them, its own checked; a pick switches that session alone (send.rs
//   `switch_model`, `switch_effort`), on whichever device it's on. Not while its turn runs or it
//   waits on you, where the command would go in as a prompt or answer it; the app takes a switch
//   whenever, for the session's next turn (`nextTurn`). The model it's on is the one its latest
//   reply came from, or that it was switched to since (`Session.model`), and none is checked: a
//   reply names the model without its context size.
// - Not running, what it goes on with (start.ts `resumeChoice`): its account's own model or one
//   of its models, the effort, and where it goes on, as a row under them; the box's placeholder
//   says what sending does then, and the send asks for it.
// - The other accounts of its harness it can move to, each a line of its own; picking one asks
//   whether to move it there (MoveAccount.tsx `MoveDialog`), whatever its turn is doing.
// With none of that, the chip only says the account, model and effort.

import { agentId, agentOptions, effortStep, modelName, parseAgent, placeTakesModel, resumeAccount, type ChosenStart, type ResumePicks } from '../start';
import { HARNESS_WORD } from '../harness';
import type { AccountMoves } from '../menus';
import type { Session, StartAccount, StartPlace } from '../types';
import { AgentFoot } from './AgentMenu';
import { BoxMenu } from './ModelMenu';
import type { PickerOption, PickerSection } from './Picker';
import { Icon } from './primitives';
import { AccountHead, agentLine } from './StartControls';

/** A running session's switches: what Codealong can switch, and how. */
export interface LiveSwitch {
  switches: boolean;
  switchesEffort: boolean;
  /** Its turn runs or it waits on you: a switch waits for it. */
  busy: boolean;
  nextTurn: boolean;
  onSwitch: (model: string) => void;
  onEffort: (effort: string) => void;
}

/** What a session that isn't running goes on with: the picks and what they came to. */
export interface ResumeSwitch {
  picks: ResumePicks;
  chosen: ChosenStart;
  onPicks: (picks: ResumePicks) => void;
}

interface SessionAgentProps {
  session: Session;
  /** Its account, as start options list it, when they're read. */
  account: StartAccount | undefined;
  live?: LiveSwitch | null;
  resume?: ResumeSwitch | null;
  /** The accounts it can move to, and asking to move it to one. */
  moves?: AccountMoves | null;
  onMove?: (to: string) => void;
  onDone: () => void;
}

/** A line that moves the session to another account: `move|<account>`, which no agent id is. */
const MOVE = 'move|';

/** Whether there's anything to pick for a session that isn't running: more than one place, or a
 *  model. */
export function offersResume(chosen: ChosenStart | null): chosen is ChosenStart {
  return !!chosen && (chosen.places.length > 1 || chosen.takesAModel);
}

export function SessionAgent({ session: s, account, live, resume, moves, onMove, onDone }: SessionAgentProps) {
  const resumed = resume && account ? resumeAccount(account) : null;
  const a = resumed ?? account;
  const models = a?.models ?? [];
  const efforts = a?.efforts ?? [];
  // What it's on, or goes on with.
  const model = resume ? resume.chosen.modelShown : s.model ? modelName(s.model, models) : null;
  const effort = resume ? resume.chosen.effortNow : (s.effort ?? null);
  const detail = [model, effort].filter(Boolean).join(' · ') || null;
  const label = <span className="ca-account-name">{s.account}</span>;
  const title = `${s.account} (${HARNESS_WORD[s.harness]})`;

  // Its own account's lines.
  const own: PickerOption[] = [];
  let value: string | null = null;
  if (resume && a && resume.chosen.takesAModel) {
    value = resume.chosen.agentNow ?? agentId(a.name, null);
    own.push(...agentOptions([a], models, null).map((o) => agentLine(o, a, models, value)));
  } else if (live?.switches && a) {
    // Only a model by its name: the account's own isn't one `/model` takes.
    own.push(
      ...agentOptions([a], models, null)
        .filter((o) => o.model)
        .map((o) => ({ ...agentLine(o, a, models, null), disabled: live.busy, title: live.busy ? 'Once its turn ends' : undefined })),
    );
  }
  const sections: PickerSection[] = [];
  if (own.length > 0) sections.push({ key: s.account, head: <AccountHead name={s.account} detail={HARNESS_WORD[s.harness]} meters={null} note="Its account" now={0} />, options: own });
  for (const to of moves && onMove ? moves.to : []) {
    sections.push({
      key: to,
      head: <AccountHead name={to} detail={HARNESS_WORD[s.harness]} meters={null} note={to === moves?.suggested ? 'Has room' : ''} now={0} />,
      options: [{ id: `${MOVE}${to}`, text: `${to} move`, label: 'Move it here', detail: 'Asks first', lead: <span className="ca-picker-check" /> }],
    });
  }

  // The rows under the lines: a running one's level, its own not among them, a pick switching it
  // and done with the menu; one that isn't running, the effort and where it goes on.
  const pickPlace = (place: StartPlace) => resume?.onPicks({ ...resume.picks, place, agent: placeTakesModel(place, true) ? resume.picks.agent : null });
  const foot = resume ? (
    <AgentFoot
      // The levels as the chip says them (`xhigh`).
      effort={efforts.length > 0 && a ? { efforts: efforts.map((e) => ({ id: e.id, label: e.id })), value: resume.picks.effort, onPick: (e) => resume.onPicks({ ...resume.picks, effort: e }), harness: a.harness, model: resume.chosen.effortModel } : null}
      place={{ places: resume.chosen.places, value: resume.chosen.placeNow, onPick: pickPlace, resuming: true }}
    />
  ) : live?.switchesEffort && !live.busy && a ? (
    (close: () => void) => (
      <AgentFoot
        effort={{
          efforts: efforts.map((e) => ({ id: e.id, label: e.id })),
          value: effort,
          running: true,
          harness: a.harness,
          model: s.model ?? null,
          onPick: (level) => {
            if (level && level !== effort) live.onEffort(level);
            close();
          },
        }}
      />
    )
  ) : null;
  const hasFoot = resume ? (efforts.length > 0 || resume.chosen.places.length > 1) : !!foot;

  if (sections.length === 0 && !hasFoot) {
    return (
      <span className="ca-modelmenu-said" title={`${title}. ${effort ? 'The model and effort its latest reply came from' : 'The model its latest reply came from'}`}>
        {label}
        {detail && <span className="ca-modelmenu-detail">{detail}</span>}
      </span>
    );
  }
  const what = live ? (live.busy ? 'Its account; its model and effort switch once its turn ends' : live.nextTurn ? 'Its account, and its model and effort from its next turn' : 'Its account, model and effort, for this session alone') : 'Its account, and the model, effort and place it goes on with';
  return (
    <BoxMenu
      kind="model"
      sections={sections}
      value={value}
      icon={undefined}
      label={label}
      detail={detail}
      title={`${title}. ${what}`}
      placeholder="Account or model"
      foot={foot}
      onStep={resume && efforts.length > 0 ? (delta) => resume.onPicks({ ...resume.picks, effort: effortStep(efforts, resume.picks.effort, delta) }) : undefined}
      onPick={(id) => {
        if (id.startsWith(MOVE)) {
          onMove?.(id.slice(MOVE.length));
          return;
        }
        const { model: picked } = parseAgent(id);
        if (resume) {
          // A model is asked for on the command line or of Codealong's adapter, so a place that
          // takes none gives way to a terminal.
          const place = picked && resume.chosen.placeNow && !placeTakesModel(resume.chosen.placeNow, true) ? 'terminal' : resume.picks.place;
          resume.onPicks({ ...resume.picks, agent: id, place });
        } else if (picked) live?.onSwitch(picked);
      }}
      onDone={onDone}
    />
  );
}

/** Where a session runs, or goes on, at the left of its box's foot. */
export function BoxPlace({ icon, label, title }: { icon: string; label: string; title: string }) {
  return (
    <span className="ca-boxplace" title={title}>
      <Icon name={icon} size={12} />
      <span>{label}</span>
    </span>
  );
}
