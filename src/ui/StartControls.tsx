// The controls for where a session goes and on what, wherever one is started or goes on: the
// quick panel's start row, and the box of a session that isn't running (ResumeBar.tsx). The
// rules for what they offer are start.ts's; these only draw them.

import type { AgentOption, ModelOption } from '../start';
import { agentOptionWords, effortNote } from '../start';
import type { Harness, StartAccount, StartPlace } from '../types';
import type { PickerOption } from './Picker';
import { cx, Icon, Kbd, Seg } from './primitives';

export function placeIcon(place: StartPlace): string {
  return place === 'terminal' ? 'terminal' : place === 'background' ? 'stack' : 'app';
}

/** Where it goes: a toggle between the places on offer, or the one place said, or nothing. */
export function PlaceToggle({ places, value, onPick, label, className, keepsFocus }: { places: StartAccount['places']; value: StartPlace | null; onPick: (p: StartPlace) => void; label: string; className?: string; keepsFocus?: boolean }) {
  if (places.length > 1) {
    return <Seg className={cx('ca-places', className)} label={label} iconSize={13} options={places.map((p) => ({ id: p.place, label: p.label, icon: placeIcon(p.place) }))} value={value} onPick={onPick} keepsFocus={keepsFocus} />;
  }
  if (places.length === 0) return null;
  return (
    <span className="ca-place">
      <Icon name={placeIcon(places[0].place)} size={13} />
      {places[0].label}
    </span>
  );
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

/** The effort row under a model menu's lines: a setting for whichever model is chosen, so it
 *  stays while you look through them. Null where the account takes no effort. */
export function EffortFoot({ efforts, value, onPick, harness, model }: { efforts: { id: string; label: string }[]; value: string | null; onPick: (e: string | null) => void; harness: Harness | null; model: string | null }) {
  if (efforts.length === 0) return null;
  return (
    <>
      <span className="ca-picker-foot-label">Effort</span>
      <Seg
        label="Effort"
        options={[{ id: null, label: 'Default', title: 'The account’s own: settings.json’s effortLevel, else the model’s' }, ...efforts.map((e) => ({ id: e.id as string | null, label: e.label, title: e.id === 'max' ? 'Deepest reasoning. It can use a great many tokens; for the hardest tasks.' : undefined }))]}
        value={value}
        // The filter keeps the keyboard.
        keepsFocus
        onPick={onPick}
      />
      <span className="ca-picker-foot-note">{effortNote(harness, model) ?? <Kbd keys={['←', '→']} />}</span>
    </>
  );
}
