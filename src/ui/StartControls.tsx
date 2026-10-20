// The controls for where a session goes and on what, wherever one is started or goes on: the
// quick panel's start row, the box of a session that isn't running (SessionAgent.tsx), and the
// agent menu everywhere else (AgentMenu.tsx). The rules for what they offer are start.ts's; these
// only draw them.

import { usageNote } from '../harness';
import type { AgentOption, AutoPick, ModelOption } from '../start';
import { accountHeadWords, agentOptionWords, autoOptionWords, effortDefaultTitle, effortNote, groupOptionWords, isAuto, pickFromGroup, placeNote } from '../start';
import type { Harness, Lane, Meter, RoutingGroup, StartAccount, StartPlace, Usage } from '../types';
import type { PickerOption, PickerSection } from './Picker';
import { Icon, Kbd, Seg } from './primitives';
import { UsageMeter } from './UsageBar';

export function placeIcon(place: StartPlace): string {
  return place === 'terminal' ? 'terminal' : place === 'background' ? 'stack' : 'app';
}

/** Where it goes, as a menu of the places on offer: each with its icon, and under its name what
 *  it is (start.ts `placeNote`). Picked seldom, so it's a chip that says the place, not a row of
 *  every place; with one place there's nothing to pick, and the words of the start or the box
 *  say where it goes. */
export function placeSections(places: StartAccount['places'], resuming = false): PickerSection[] {
  return [
    {
      key: 'places',
      options: places.map((p) => {
        const note = placeNote(p.place, p.behind, resuming);
        return { id: p.place, text: `${p.label} ${note}`, label: p.label, detail: note, lead: <Icon name={placeIcon(p.place)} size={14} /> };
      }),
    },
  ];
}

/** One of an account's lines in a model menu: its own model or one of its models, checked when
 *  it's the one chosen. */
export function agentLine(o: AgentOption, a: StartAccount, models: ModelOption[], chosen: string | null): PickerOption {
  const said = agentOptionWords(o, a, models);
  const cant = a.places.length === 0;
  return {
    id: o.id,
    lead: <span className="ca-picker-check">{o.id === chosen ? <Icon name="check" size={14} /> : null}</span>,
    text: said.text,
    label: said.label,
    detail: said.detail,
    disabled: cant,
    title: cant ? (a.unavailable ?? undefined) : undefined,
  };
}

/** What the deepest levels are, on hover: they can use a great many tokens. */
const LEVEL_TITLES: Record<string, string> = {
  max: 'Maximum reasoning depth. It can use a great many tokens; for the hardest tasks.',
  ultra: 'Maximum reasoning, with Codex handing parts of the task to agents of its own. It can use a great many tokens.',
};

/** The effort row under a model menu's lines: a setting for whichever model is chosen, so it
 *  stays while you look through them, or (`running`) a running session's level, which a pick
 *  switches, the account's own not being one to switch to. Null where no account takes effort;
 *  `takesEffort` is whether the chosen one does. */
export function EffortFoot({ efforts, value, onPick, harness, model, running = false, takesEffort = true, keys = true }: { efforts: { id: string; label: string }[]; value: string | null; onPick: (e: string | null) => void; harness: Harness | null; model: string | null; running?: boolean; takesEffort?: boolean; /** ← → step it; false where they step another row. */ keys?: boolean }) {
  if (efforts.length === 0) return null;
  const levels = efforts.map((e) => ({ id: e.id as string | null, label: e.label, title: LEVEL_TITLES[e.id] }));
  return (
    <>
      <span className="ca-picker-foot-label">Effort</span>
      <Seg
        label="Effort"
        options={running ? levels : [{ id: null, label: 'Default', title: effortDefaultTitle(harness) }, ...levels]}
        value={value}
        // The filter keeps the keyboard.
        keepsFocus
        onPick={onPick}
      />
      <span className="ca-picker-foot-note">{effortNote(harness, model, takesEffort) ?? (running ? 'This session only' : keys ? <Kbd keys={['←', '→']} /> : null)}</span>
    </>
  );
}

/** An account's heading in an agent menu: its name, what it is, and its meters, or why there are
 *  none. */
export function AccountHead({ name, detail, meters, note, now }: { name: string; detail: string; meters: Meter[] | null; note: string; now: number }) {
  return (
    <span className="ca-quick-acct">
      <span className="ca-quick-acct-name">{name}</span>
      <span className="ca-quick-acct-detail">{detail}</span>
      <span className="ca-quick-acct-meters">{meters && meters.length > 0 ? meters.map((m) => <UsageMeter key={m.key} meter={m} now={now} />) : <span className="ca-pick-note">{note}</span>}</span>
    </span>
  );
}

/**
 * The agent menu, as the quick panel starts a session with it and AgentMenu.tsx offers it with
 * Auto (an automation's agent, who answers questions about a PR):
 * Auto and any routing groups first where they're offered, saying whom they would start now
 * (`autoTitle` says what Auto means there), then each account under its heading with its usage,
 * each with its own model and its models, `chosen` checked.
 */
export function agentMenuSections(agents: AgentOption[], accounts: StartAccount[], pick: AutoPick | null, usage: Usage[], models: ModelOption[], now: number, chosen: string | null, autoTitle: string, groups: RoutingGroup[] = [], lanes: Lane[] = []): PickerSection[] {
  const out: PickerSection[] = [];
  const autoOptions: PickerOption[] = [];
  const auto = agents.find((o) => isAuto(o.choice));
  if (auto && pick) {
    const said = autoOptionWords(pick, accounts, usage, models, now);
    autoOptions.push({ id: auto.id, text: said.text, label: 'Auto', detail: said.detail, lead: <Icon name="sparkles" size={14} />, title: autoTitle });
  }
  for (const g of groups) {
    const opt = agents.find((o) => o.choice === g.name);
    const gp = pickFromGroup(g, accounts, usage, now, null, null, lanes);
    if (opt && gp) {
      const said = groupOptionWords(g, gp, accounts, usage, models, now);
      autoOptions.push({ id: opt.id, text: said.text, label: g.name, detail: said.detail, lead: <Icon name="sparkles" size={14} />, title: `Routing group (${g.strategy})` });
    } else {
      autoOptions.push({ id: g.name, text: `${g.name} routing group`, label: g.name, detail: 'No account in this group can start here', lead: <Icon name="sparkles" size={14} />, disabled: true, title: `Routing group (${g.strategy})` });
    }
  }
  if (autoOptions.length > 0) out.push({ key: 'auto', options: autoOptions });
  for (const a of accounts) {
    const u = usage.find((x) => x.account === a.name);
    const mine = agents.filter((o) => !isAuto(o.choice) && o.choice === a.name);
    const cant = a.places.length === 0;
    const head = accountHeadWords(a, u, usageNote(u));
    out.push({
      key: a.name,
      head: <AccountHead name={a.name} detail={head.detail} meters={cant ? null : (u?.meters ?? [])} note={head.note} now={now} />,
      options: mine.map((o) => agentLine(o, a, models, chosen)),
    });
  }
  return out;
}
