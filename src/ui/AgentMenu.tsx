// An account with a model of its and an effort, picked under one chip: the agent menu's accounts
// with their models (StartControls.tsx `agentMenuSections`) and the rows under them
// (`AgentFoot`), in a box menu (ModelMenu.tsx `BoxMenu`). Each member of a routing group is one,
// and so is what adds one (SettingsDialog.tsx `RoutingPane`). Where Auto and the routing groups are
// offered too, the quick panel's start row and an automation's agent (AgentPromptBox.tsx) build the
// same sections themselves, and the quick panel's menu has the same rows.
//
// No model is Auto's: the account's own, or the best other with room once that one's own limit is
// nearly used (start.ts `autoModel`, start.rs `auto_model`), so its line says Auto.
//
// The rows under the lines are the effort (StartControls.tsx `EffortFoot`) and, where a start
// picks one, where it runs (`PlaceRow`): each account has a place its sessions start in (its
// `start` in config.toml, Settings › Accounts), which a start goes by unless another is picked for
// it alone; Make default keeps the one picked as the account's (asked 2026-10-19).

import { useMemo, type ComponentProps } from 'react';
import { agentChipWords, agentId, agentOptions, effortStep, modelName, ownModels, parseAgent, placeNote, takesModel } from '../start';
import type { StartAccount, StartPlace, Usage } from '../types';
import { BoxMenu } from './ModelMenu';
import type { PickerSection } from './Picker';
import { Seg } from './primitives';
import { agentMenuSections, EffortFoot } from './StartControls';

/** Where a start runs, among the places its account offers (`places`, each a Seg option with what
 *  it is on hover), and the account's own place (`usual`): where it starts unless another is
 *  picked, which Make default sets to the one picked (`onKeep`). No `usual` where it isn't this
 *  Mac's to keep (a start on another Mac). */
export interface PlaceRow {
  places: StartAccount['places'];
  value: StartPlace | null;
  onPick: (place: StartPlace) => void;
  /** Whether it goes on in a place rather than starts there (a resume), as `placeNote` says it. */
  resuming?: boolean;
  usual?: {
    place: StartPlace | null;
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
          <EffortFoot {...effort} />
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
          {kept && placed.value && placed.value !== kept.place ? (
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

/** An account, its model (null for Auto's) and its effort (null for the account's own). */
export interface AgentPick {
  account: string;
  model: string | null;
  effort: string | null;
}

export interface AgentMenuProps {
  /** The accounts it offers, as start options list them. */
  accounts: StartAccount[];
  usage: Usage[];
  now: number;
  /** The pick the chip says and the menu checks, with its effort row; null for none yet. */
  value: AgentPick | null;
  /** What the chip says, and its icon, with no pick. */
  label?: string;
  icon?: string;
  title: string;
  disabled?: boolean;
  /** Opens under the chip, where what scrolls cuts off the room above it (Settings). */
  down?: boolean;
  onPick: (pick: AgentPick) => void;
}

const AUTO_SAYS = 'Its default, or the next best once its own limit is nearly used';

/** The agent menu's account lines, each account's own model said as Auto's. */
export function agentPickSections(accounts: StartAccount[], usage: Usage[], now: number, chosen: string | null): PickerSection[] {
  const models = accounts.flatMap((a) => a.models ?? []).filter((m, i, all) => all.findIndex((x) => x.id === m.id) === i);
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

export function AgentMenu({ accounts, usage, now, value, label = 'Account or model', icon, title, disabled, down, onPick }: AgentMenuProps) {
  const chosen = value ? agentId(value.account, value.model) : null;
  const sections = useMemo(() => agentPickSections(accounts, usage, now, chosen), [accounts, usage, now, chosen]);
  const a = value ? accounts.find((x) => x.name === value.account) : undefined;
  const efforts = a?.efforts ?? [];
  const models = ownModels(a, accounts.flatMap((x) => x.models ?? []));
  const said = value
    ? agentChipWords({
        auto: false,
        account: value.account,
        accounts: accounts.length,
        remote: null,
        model: value.model ? modelName(value.model, models) : a && takesModel(a) ? 'Auto' : null,
        effortWord: value.effort ? (efforts.find((e) => e.id === value.effort)?.label ?? value.effort) : null,
      })
    : null;
  const foot = value && a && efforts.length > 0 ? <AgentFoot effort={{ efforts, value: value.effort, onPick: (effort) => onPick({ ...value, effort }), harness: a.harness, model: value.model ?? a.defaultModel }} /> : null;
  return (
    <BoxMenu
      kind="model"
      sections={sections}
      value={chosen}
      icon={value ? undefined : icon}
      label={said?.label ?? label}
      detail={value && !a ? 'Not an account that can start here' : said?.detail || null}
      title={title}
      disabled={disabled}
      placeholder="Account or model"
      foot={foot}
      onStep={foot && value ? (delta) => onPick({ ...value, effort: effortStep(efforts, value.effort, delta) }) : undefined}
      onPick={(id) => {
        const { choice, model } = parseAgent(id);
        // Another account takes its own effort; another model of the same keeps the one set.
        onPick({ account: choice, model, effort: choice === value?.account ? value.effort : null });
      }}
      onDone={() => {}}
      down={down}
    />
  );
}
