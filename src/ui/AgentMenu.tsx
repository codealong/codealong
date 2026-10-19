// An account with a model of its and an effort, picked under one chip: the agent menu's accounts
// with their models (StartControls.tsx `agentMenuSections`) and the effort row under them
// (`EffortFoot`), in a box menu (ModelMenu.tsx `BoxMenu`). Each member of a routing group is one,
// and so is what adds one (SettingsDialog.tsx `RoutingPane`). Where Auto and the routing groups are
// offered too, the quick panel's start row and an automation's agent (AgentPromptBox.tsx) build the
// same sections themselves.
//
// No model is Auto's: the account's own, or the best other with room once that one's own limit is
// nearly used (start.ts `autoModel`, start.rs `auto_model`), so its line says Auto.

import { useMemo } from 'react';
import { agentChipWords, agentId, agentOptions, effortStep, modelName, ownModels, parseAgent, takesModel } from '../start';
import type { StartAccount, Usage } from '../types';
import { BoxMenu } from './ModelMenu';
import type { PickerSection } from './Picker';
import { agentMenuSections, EffortFoot } from './StartControls';

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
  const foot = value && a && efforts.length > 0 ? <EffortFoot efforts={efforts} value={value.effort} onPick={(effort) => onPick({ ...value, effort })} harness={a.harness} model={value.model ?? a.defaultModel} /> : null;
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
