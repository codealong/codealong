// A session's account, model and effort, as one chip at the right of its box (SessionReply.tsx),
// beside Send: the agent menu's account lines and the rows under them (AgentMenu.tsx `AgentFoot`)
// in a box menu (ModelMenu.tsx `BoxMenu`), as the quick panel starts one with (asked 2026-10-19).
//
// What its menu has goes by what can be done with the session:
// - Running, its account's models where Codealong can switch them (one it runs, through its agent;
//   one whose terminal or background job's box it types into, by its harness's commands
//   (SessionReply.tsx `typesCommands`), `StartAccount.switchesModel` and
//   `switchesEffort`; one whose app goes on with it, through the app, `switchesInApp`), and the
//   levels under them, its own checked; a pick switches that session alone (send.rs
//   `switch_model`, `switch_effort`), on whichever device it's on.
// - Not running, what it goes on with (start.ts `resumeChoice`): its account's own model or one
//   of its models, the effort, and where it goes on, as a row under them; the box's placeholder
//   says what sending does then, and the send asks for it.
// - Any other signed-in account on this Mac (same harness or cross-provider), with its own models
//   and default: picking another account stages a move or handoff right in the reply box
//   (HandoffStrip.tsx), where the chip shows the staged target and lets you adjust its model,
//   effort, and where it opens before confirming.
// With none of that, the chip only says the account, model and effort.

import type { MoveTarget, StagedHandoff } from '../handoff';
import { HARNESS_WORD } from '../harness';
import type { AccountMoves } from '../menus';
import { agentId, agentOptions, effortStep, modelName, parseAgent, placeTakesModel, resumeAccount, takesModel, type ChosenStart, type ResumePicks } from '../start';
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
  /** Why it can't be switched now, as a sentence ends (`once its turn ends`), or null when it
   *  can: it waits on you, a switch is under way, or its turn runs where a command typed would
   *  wait for it. */
  busy: string | null;
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

/** A move or handoff staged on the reply box for another account or provider. */
export interface HandoffSwitch {
  staged: StagedHandoff;
  target: MoveTarget;
  account: StartAccount | undefined;
  onChange: (next: StagedHandoff | null) => void;
}

interface SessionAgentProps {
  session: Session;
  /** Its account, as start options list it, when they're read. */
  account: StartAccount | undefined;
  /** Every startable account on the session's device, for target accounts' models and places. */
  accounts?: StartAccount[];
  live?: LiveSwitch | null;
  resume?: ResumeSwitch | null;
  /** The accounts it can move or hand off to. */
  moves?: AccountMoves | null;
  /** Active staged move/handoff in the reply box, when switched to another account. */
  handoff?: HandoffSwitch | null;
  /** Stages a move/handoff to `account` (with optional `model`) right in the reply box. */
  onStage?: (account: string, model: string | null) => void;
  onDone: () => void;
}

/** A line that moves or hands off to another account: `:move:<account>` or `:move:<account>|<model>`,
 *  which no agent id can be, an account's name being letters, numbers, dots, dashes and underscores. */
const MOVE = ':move:';
/** A line that cancels a staged move and returns to the session's own account. */
const STAY = ':stay:';

/** Parses a move/handoff picker option id into its target account and optional model, or null for
 *  a line of the session's own account. */
export function parseMovePick(id: string): { account: string; model: string | null } | null {
  if (!id.startsWith(MOVE)) return null;
  const rest = id.slice(MOVE.length);
  const at = rest.indexOf('|');
  return at < 0 ? { account: rest, model: null } : { account: rest.slice(0, at), model: rest.slice(at + 1) || null };
}

/** The account a line of the chip's menu moves the session to, or null for a line of its own
 *  account's (`<account>|<model>`, whatever the account is named). */
export function moveTarget(id: string): string | null {
  return parseMovePick(id)?.account ?? null;
}

/** A phrase as a sentence of its own: `once its turn ends` as `Once its turn ends`. */
function sentence(phrase: string): string {
  return phrase.charAt(0).toUpperCase() + phrase.slice(1);
}

/** Whether there's anything to pick for a session that isn't running: more than one place, or a
 *  model. */
export function offersResume(chosen: ChosenStart | null): chosen is ChosenStart {
  return !!chosen && (chosen.places.length > 1 || chosen.takesAModel);
}

export function SessionAgent({ session: s, account, accounts = [], live, resume, moves, handoff, onStage, onDone }: SessionAgentProps) {
  const resumed = resume && account ? resumeAccount(account) : null;
  const a = resumed ?? account;
  const models = a?.models ?? [];
  const efforts = a?.efforts ?? [];

  // Target account's config when a move/handoff is staged.
  const ha = handoff?.account;
  const hPlaces = ha?.places ?? [];
  const hPlaceNow: StartPlace | null = handoff
    ? handoff.staged.place && hPlaces.some((p) => p.place === handoff.staged.place)
      ? handoff.staged.place
      : (ha?.chosen ?? hPlaces[0]?.place ?? null)
    : null;
  const hTakesModel = hPlaceNow ? placeTakesModel(hPlaceNow, false) : true;
  const hModels = ha?.models ?? [];
  const hEfforts = ha?.efforts ?? [];

  // What the chip displays: staged target when switching accounts, else the session's own.
  const model = handoff
    ? hTakesModel
      ? handoff.staged.model
        ? modelName(handoff.staged.model, hModels)
        : ha?.defaultModel
          ? modelName(ha.defaultModel, hModels)
          : null
      : null
    : resume
      ? resume.chosen.modelShown
      : s.model
        ? modelName(s.model, models)
        : null;
  const effort = handoff ? (hTakesModel ? handoff.staged.effort : null) : resume ? resume.chosen.effortNow : (s.effort ?? null);
  const detail = [model, effort].filter(Boolean).join(' · ') || null;
  const label = <span className="ca-account-name">{handoff ? `→ ${handoff.staged.account}` : s.account}</span>;
  const title = handoff ? `Switching to ${handoff.staged.account} (${HARNESS_WORD[handoff.target.harness]})` : `${s.account} (${HARNESS_WORD[s.harness]})`;

  // Its own account's lines.
  const own: PickerOption[] = [];
  let value: string | null = handoff ? `${MOVE}${handoff.staged.account}|${handoff.staged.model ?? ''}` : null;
  if (handoff) {
    own.push({
      id: STAY,
      text: `${s.account} ${HARNESS_WORD[s.harness]} stay cancel`,
      label: `Stay on ${s.account}`,
      detail: 'Cancel move',
      lead: <span className="ca-picker-check" />,
    });
  }
  if (resume && a && resume.chosen.takesAModel) {
    if (!handoff) value = resume.chosen.agentNow ?? agentId(a.name, null);
    own.push(...agentOptions([a], models, null).map((o) => agentLine(o, a, models, handoff ? null : value)));
  } else if (live?.switches && a) {
    // Only a model by its name: the account's own isn't one `/model` takes.
    own.push(
      ...agentOptions([a], models, null)
        .filter((o) => o.model)
        .map((o) => ({ ...agentLine(o, a, models, null), disabled: !!live.busy, title: live.busy ? sentence(live.busy) : undefined })),
    );
  }

  const sections: PickerSection[] = [];
  if (own.length > 0) sections.push({ key: s.account, head: <AccountHead name={s.account} detail={HARNESS_WORD[s.harness]} meters={null} note="Its account" now={0} />, options: own });
  for (const t of moves && onStage ? moves.targets : []) {
    const ta = accounts.find((x) => x.name === t.account && x.harness === t.harness);
    const tModels = ta && takesModel(ta) ? (ta.models ?? []) : [];
    const actionWord = t.native ? 'Move it here' : 'Hand off here';
    const defId = `${MOVE}${t.account}|`;
    const isDef = value === defId;
    const opts: PickerOption[] = [];
    if (tModels.length > 0) {
      const defLabel = ta?.defaultModel ? `Default · ${modelName(ta.defaultModel, tModels)}` : 'Default';
      opts.push({
        id: defId,
        text: `${t.account} ${HARNESS_WORD[t.harness]} default ${defLabel} move handoff`,
        label: defLabel,
        detail: actionWord,
        lead: <span className="ca-picker-check">{isDef ? <Icon name="check" size={14} /> : null}</span>,
      });
      for (const m of tModels) {
        const mid = `${MOVE}${t.account}|${m.id}`;
        opts.push({
          id: mid,
          text: `${t.account} ${HARNESS_WORD[t.harness]} ${m.label} ${m.id} move handoff`,
          label: m.label,
          detail: actionWord,
          lead: <span className="ca-picker-check">{value === mid ? <Icon name="check" size={14} /> : null}</span>,
        });
      }
    } else {
      opts.push({
        id: defId,
        text: `${t.account} ${HARNESS_WORD[t.harness]} move handoff`,
        label: actionWord,
        detail: t.native ? 'Closes & archives this' : 'With handoff brief',
        lead: <span className="ca-picker-check">{isDef ? <Icon name="check" size={14} /> : null}</span>,
      });
    }
    sections.push({
      key: t.account,
      head: <AccountHead name={t.account} detail={HARNESS_WORD[t.harness]} meters={null} note={t.account === moves?.suggested ? 'Has room' : ''} now={0} />,
      options: opts,
    });
  }

  // Footer rows under the picker: staged target's effort & place when switching accounts, else the
  // session's own resume/live effort & place.
  const pickPlace = (place: StartPlace) => resume?.onPicks({ ...resume.picks, place, agent: placeTakesModel(place, true) ? resume.picks.agent : null });
  const pickHandoffPlace = (place: StartPlace) => {
    if (!handoff) return;
    const keepsModel = placeTakesModel(place, false);
    handoff.onChange({
      ...handoff.staged,
      place,
      model: keepsModel ? handoff.staged.model : null,
      effort: keepsModel ? handoff.staged.effort : null,
    });
  };
  const foot = handoff ? (
    ha && (hEfforts.length > 0 || hPlaces.length > 1) ? (
      <AgentFoot
        effort={
          hEfforts.length > 0
            ? {
                efforts: hEfforts.map((e) => ({ id: e.id, label: e.id })),
                value: handoff.staged.effort,
                onPick: (e) => handoff.onChange({ ...handoff.staged, effort: e }),
                harness: ha.harness,
                model: handoff.staged.model ?? ha.defaultModel,
                takesEffort: hTakesModel,
              }
            : null
        }
        place={{ places: hPlaces, value: hPlaceNow, onPick: pickHandoffPlace }}
      />
    ) : null
  ) : resume ? (
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
  const hasFoot = handoff ? !!foot : resume ? efforts.length > 0 || resume.chosen.places.length > 1 : !!foot;

  if (sections.length === 0 && !hasFoot) {
    return (
      <span className="ca-modelmenu-said" title={`${title}. ${effort ? 'The model and effort its latest reply came from' : 'The model its latest reply came from'}`}>
        {label}
        {detail && <span className="ca-modelmenu-detail">{detail}</span>}
      </span>
    );
  }
  const what = handoff
    ? 'Staged move or handoff; pick another account, model, effort or place, or stay on this session'
    : live
      ? live.busy
        ? `Its account; its model and effort can be switched ${live.busy}`
        : live.nextTurn
          ? 'Its account, and its model and effort from its next turn'
          : 'Its account, model and effort, for this session alone'
      : 'Its account, and the model, effort and place it goes on with';
  return (
    <BoxMenu
      kind="model"
      chipClass={handoff ? 'is-handoff' : undefined}
      sections={sections}
      value={value}
      icon={undefined}
      label={label}
      detail={detail}
      title={`${title}. ${what}`}
      placeholder="Account or model"
      foot={foot}
      onStep={
        handoff && hEfforts.length > 0
          ? (delta) => handoff.onChange({ ...handoff.staged, effort: effortStep(hEfforts, handoff.staged.effort, delta) })
          : resume && efforts.length > 0
            ? (delta) => resume.onPicks({ ...resume.picks, effort: effortStep(efforts, resume.picks.effort, delta) })
            : undefined
      }
      onPick={(id) => {
        if (id === STAY) {
          handoff?.onChange(null);
          return;
        }
        const mv = parseMovePick(id);
        if (mv !== null) {
          onStage?.(mv.account, mv.model);
          return;
        }
        if (handoff) handoff.onChange(null);
        const { model: picked } = parseAgent(id);
        if (resume) {
          // A model is asked for on the command line or of Codealong's adapter, so a place that
          // takes none gives way to the default place that does.
          const place = picked && resume.chosen.placeNow && !placeTakesModel(resume.chosen.placeNow, true) ? null : resume.picks.place;
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
