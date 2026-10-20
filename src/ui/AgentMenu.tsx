// An account with a model of its and an effort, picked under one chip: the agent menu's accounts
// with their models (StartControls.tsx `agentMenuSections`) and the rows under them
// (`AgentFoot`), in a box menu (ModelMenu.tsx `BoxMenu`). It's every chip that picks an account
// and what it runs on: each member of a routing group and what adds one (SettingsDialog.tsx
// `RoutingPane`), an automation's agent (AgentPromptBox.tsx), and who answers questions about a PR
// (PrBox.tsx), the last two with Auto and the routing groups over the accounts (`AutoOffer`). The
// quick panel's start row opens the same sections and rows in a picker of its own, which its keys
// drive, and a session's box has its own chip (SessionAgent.tsx) with the same lines and rows,
// what its menu offers going by what can be done with the session, another account being a move.
//
// No model is Auto's: the account's own, or the best other with room once that one's own limit is
// nearly used (start.ts `autoModel`, start.rs `auto_model`), so its line says Auto.
//
// The rows under the lines are the effort (StartControls.tsx `EffortFoot`) and, where a start
// picks one, where it runs (`PlaceRow`): each account has a place its sessions start in (its
// `start` in config.toml, Settings › Accounts), which a start goes by unless another is picked for
// it alone; Make default keeps the one picked as the account's (asked 2026-10-19).

import { useMemo, type ComponentProps } from 'react';
import { agentChipWords, agentId, agentOptions, autoPick, effortStep, isAuto, isGroup, modelName, ownModels, parseAgent, pickFromGroup, placeNote, takesModel, type ModelOption } from '../start';
import type { Lane, RoutingGroup, StartAccount, StartPlace, Usage } from '../types';
import { BoxMenu } from './ModelMenu';
import type { PickerSection } from './Picker';
import { Seg } from './primitives';
import { Kbd } from './primitives';
import { agentMenuSections, EffortFoot } from './StartControls';

/** Where a start runs, among the places its account offers (`places`, each a Seg option with what
 *  it is on hover), and the accounts' own place (`usual`): where they start unless another is
 *  picked, which Make default sets to the one picked (`onKeep`). No `usual` where it isn't this
 *  Mac's to keep (a start on another Mac). */
export interface PlaceRow {
  places: StartAccount['places'];
  value: StartPlace | null;
  onPick: (place: StartPlace) => void;
  /** Whether it goes on in a place rather than starts there (a resume), as `placeNote` says it. */
  resuming?: boolean;
  /** ← → step it (the menu opened for it, ⌘O) rather than the effort. */
  stepped?: boolean;
  usual?: {
    /** Every account Make default would set starts in the one picked already (start.ts `keptAt`). */
    kept: boolean;
    /** Whose it is, as the button says it: an account's name, or `their` for several. */
    whose: string;
    onKeep: () => void;
  };
}

/** The rows under an agent menu's lines: its effort and where it runs, either left out where
 *  there's none to pick. */
export function AgentFoot({ effort, place }: { effort?: ComponentProps<typeof EffortFoot> | null; place?: PlaceRow | null }) {
  const placed = place && place.places.length > 1 ? place : null;
  if ((!effort || effort.efforts.length === 0) && !placed) return null;
  const kept = placed?.usual;
  return (
    <div className="ca-picker-foot-rows">
      {effort && effort.efforts.length > 0 && (
        <div className="ca-picker-foot-row">
          <EffortFoot {...effort} keys={!placed?.stepped} />
        </div>
      )}
      {placed && (
        <div className="ca-picker-foot-row">
          <span className="ca-picker-foot-label">Runs in</span>
          <Seg
            label="Runs in"
            options={placed.places.map((p) => ({ id: p.place, label: p.label, title: placeNote(p.place, p.behind, placed.resuming) }))}
            value={placed.value}
            keepsFocus
            onPick={(p) => placed.onPick(p)}
          />
          {placed.stepped && <Kbd keys={['←', '→']} />}
          {kept && placed.value && !kept.kept ? (
            <button type="button" className="ca-linkbtn ca-picker-foot-note" title={`Start ${kept.whose === 'their' ? 'their' : `${kept.whose}’s`} sessions here unless you pick another`} onMouseDown={(e) => e.preventDefault()} onClick={kept.onKeep}>
              Make default
            </button>
          ) : (
            kept && <span className="ca-picker-foot-note">{kept.whose === 'their' ? 'Their default' : `${kept.whose}’s default`}</span>
          )}
        </div>
      )}
    </div>
  );
}

/** An account, its model (null for Auto's, or the account's own) and its effort (null for the
 *  account's own). Where Auto and the routing groups are offered (`AutoOffer`), `account` can be
 *  `AUTO` or a group's name, with no model or effort of its own. */
export interface AgentPick {
  account: string;
  model: string | null;
  effort: string | null;
}

/** Auto and the routing groups, offered over the accounts: by an automation's agent
 *  (AgentPromptBox.tsx) and by who answers questions about a PR (PrBox.tsx). An account's own
 *  model is its default there, Auto being a line of its own. */
export interface AutoOffer {
  /** What Auto means here, on its line. */
  title: string;
  /** What the chip says Auto or a group starts where that isn't one account for every run
   *  (another harness than each PR's session's); else the account it would start now. */
  says?: string;
  groups?: RoutingGroup[];
  /** The routing group Auto goes through (`[routing] auto`), when set. */
  group?: RoutingGroup | null;
  /** Recent lanes, so alternate-harness and round-robin groups reflect recent starts. */
  lanes?: Lane[];
}

export interface AgentMenuProps {
  /** The accounts it offers, as start options list them. */
  accounts: StartAccount[];
  usage: Usage[];
  now: number;
  /** The pick the chip says and the menu checks, with its effort row; null for none yet. */
  value: AgentPick | null;
  /** Auto and the routing groups first, where they're offered. */
  auto?: AutoOffer;
  /** What the chip says, and its icon, with no pick. */
  label?: string;
  icon?: string;
  title: string;
  disabled?: boolean;
  /** Opens under the chip, where what scrolls cuts off the room above it (Settings). */
  down?: boolean;
  onPick: (pick: AgentPick) => void;
  /** The menu closed, picked or not: a box takes the keyboard back. */
  onDone?: () => void;
}

const AUTO_SAYS = 'Its default, or the next best once its own limit is nearly used';

/** The agent menu's account lines, each account's own model said as Auto's. */
export function agentPickSections(accounts: StartAccount[], usage: Usage[], now: number, chosen: string | null): PickerSection[] {
  const models = allModels(accounts);
  return agentMenuSections(agentOptions(accounts, models, null), accounts, null, usage, models, now, chosen, '').map((s) => ({
    ...s,
    options: s.options.map((o) => {
      const { choice, model } = parseAgent(o.id);
      const a = accounts.find((x) => x.name === choice);
      if (model !== null || !a || !takesModel(a)) return o;
      return { ...o, label: `Auto${a.defaultModel ? ` · ${modelName(a.defaultModel, ownModels(a, models))}` : ''}`, detail: AUTO_SAYS };
    }),
  }));
}

/** Every account's models, each once. */
function allModels(accounts: StartAccount[]): ModelOption[] {
  return accounts.flatMap((a) => a.models ?? []).filter((m, i, all) => all.findIndex((x) => x.id === m.id) === i);
}

export function AgentMenu({ accounts, usage, now, value, auto, label = 'Account or model', icon, title, disabled, down, onPick, onDone }: AgentMenuProps) {
  const models = useMemo(() => allModels(accounts), [accounts]);
  const groups = auto?.groups;
  const autoGroup = auto?.group;
  const lanes = auto?.lanes;
  const offers = !!auto;
  const autoTitle = auto?.title ?? '';
  // Whom Auto would start now, where it's offered.
  const pick = useMemo(() => (offers ? autoPick(accounts, usage, now, null, autoGroup, lanes) : null), [offers, accounts, usage, now, autoGroup, lanes]);
  const group = value ? isGroup(value.account, groups) : undefined;
  const automatic = !!value && (isAuto(value.account) || !!group);
  const chosen = value ? agentId(value.account, value.model, groups) : null;
  const sections = useMemo(
    () => (offers ? agentMenuSections(agentOptions(accounts, models, pick, groups, usage, now, lanes), accounts, pick, usage, models, now, chosen, autoTitle, groups, lanes) : agentPickSections(accounts, usage, now, chosen)),
    [offers, accounts, models, pick, groups, usage, now, lanes, chosen, autoTitle],
  );
  // Auto and a group say whom they'd start now, and an account its own model: its default where
  // Auto is a line of its own, else Auto's.
  const picked = group ? pickFromGroup(group, accounts, usage, now, null, null, lanes) : value && isAuto(value.account) ? pick : null;
  const a = accounts.find((x) => x.name === (automatic ? picked?.account : value?.account));
  const efforts = a?.efforts ?? [];
  const named = automatic ? (picked?.model ?? null) : (value?.model ?? null);
  const said = value
    ? agentChipWords({
        auto: !!value && isAuto(value.account),
        group: group?.name ?? null,
        account: a?.name ?? (automatic ? null : value.account),
        accounts: accounts.length,
        remote: null,
        model: named ? modelName(named, ownModels(a, models)) : !offers && a && takesModel(a) ? 'Auto' : null,
        effortWord: value.effort ? (efforts.find((e) => e.id === value.effort)?.label ?? value.effort) : null,
      })
    : null;
  // Auto and a group take the effort of whom they start: only an account has one picked here.
  const foot = value && !automatic && a && efforts.length > 0 ? <AgentFoot effort={{ efforts, value: value.effort, onPick: (effort) => onPick({ ...value, effort }), harness: a.harness, model: value.model ?? a.defaultModel }} /> : null;
  return (
    <BoxMenu
      kind="model"
      sections={sections}
      value={chosen}
      icon={automatic ? 'sparkles' : value ? undefined : icon}
      label={said?.label ?? label}
      detail={value && !automatic && !a ? 'Not an account that can start here' : automatic && auto?.says ? auto.says : said?.detail || null}
      title={title}
      disabled={disabled}
      placeholder="Account or model"
      foot={foot}
      onStep={foot && value ? (delta) => onPick({ ...value, effort: effortStep(efforts, value.effort, delta) }) : undefined}
      onPick={(id) => {
        const { choice, model } = parseAgent(id);
        // Another account takes its own effort; another model of the same keeps the one set.
        onPick({ account: choice, model, effort: choice === value?.account && !automatic ? value.effort : null });
      }}
      onDone={onDone ?? (() => {})}
      down={down}
    />
  );
}
