// How busy a machine is, in a word and a colored dot (src/machineLoad.ts): in the usage bar while
// this machine is strained, where it says "Machine strained" so it isn't taken for a plan's usage,
// on each linked machine's card in Settings › Devices, and beside each machine in the quick panel's
// machine menu. Resting the pointer on the usage bar's, or a Tab to it, shows a card (HoverCard.tsx):
// what its level means, its CPUs and memory, and how busy your other machines are, for choosing
// where to start something. The others have their numbers in a tooltip (`plain`): a card in the
// menu would cover the rows you pick from, and Settings is a dialog, drawn over the cards.

import { chordSigns } from '../platform';
import { CALMER_CHORD, LOAD_WORDS, loadRows, loadSays, loadSentence } from '../machineLoad';
import type { DeviceView, Load } from '../types';
import { HoverCard, useHoverCard, type HoverCardState } from './HoverCard';
import { cx, Icon } from './primitives';

interface LoadTagProps {
  load: Load;
  name: string;
  /** The usage bar's look: an alert's tag that names the machine. */
  bar?: boolean;
  /** A tooltip, not a card. */
  plain?: boolean;
  /** Your other machines, for this machine's card to say how busy each is. */
  others?: readonly DeviceView[];
}

export function LoadTag({ load, name, bar, plain, others }: LoadTagProps) {
  const card = useHoverCard();
  return (
    <>
      <span
        ref={(el) => void (card.anchor.current = el)}
        className={cx('ca-load', `is-${load.level}`, bar && 'is-bar')}
        title={plain ? loadSentence(load, name) : undefined}
        // Its card opens from the keyboard too, as the usage bar's chips do: on a Tab to it.
        tabIndex={plain ? undefined : 0}
        aria-label={plain ? undefined : loadSentence(load, name)}
        onMouseEnter={plain ? undefined : card.show}
        onMouseLeave={plain ? undefined : card.hide}
        onFocus={plain ? undefined : (e) => e.currentTarget.matches(':focus-visible') && card.show()}
        onBlur={plain ? undefined : card.hide}
      >
        {bar ? <Icon name="gauge" size={13} /> : <i className="ca-load-dot" aria-hidden="true" />}
        {bar ? `Machine ${load.level}` : LOAD_WORDS[load.level]}
      </span>
      {card.open && <LoadCard card={card} load={load} name={name} others={others} />}
    </>
  );
}

function LoadCard({ card, load, name, others }: { card: HoverCardState; load: Load; name: string; others?: readonly DeviceView[] }) {
  const rows = loadRows(load);
  const listed = others ?? [];
  const calmOne = load.level === 'strained' && listed.some((d) => d.online && d.remoteControl && d.load?.level === 'calm');
  return (
    <HoverCard card={card} label={`${name}: ${load.level}`} className="ca-loadcard">
      <div className="ca-hovercard-head">
        <span className={cx('ca-load', `is-${load.level}`)}>
          <i className="ca-load-dot" aria-hidden="true" />
        </span>
        <span className="ca-loadcard-title">
          {name} is {load.level}
        </span>
      </div>
      <p className="ca-hovercard-meta">{loadSays(load)}</p>
      <dl className="ca-loadcard-rows">
        <dt>Load</dt>
        <dd>{rows.cpu}</dd>
        <dt>Memory</dt>
        <dd>{rows.memory}</dd>
      </dl>
      {listed.length > 0 && (
        <div className="ca-hovercard-sec">
          <span className="ca-label">Your other machines</span>
          {listed.map((d) => (
            <div key={d.key} className="ca-loadcard-machine">
              <i className={`ca-labeldot is-${d.color}`} aria-hidden="true" />
              <span className="ca-loadcard-name">{d.name}</span>
              {d.online && d.load ? (
                <LoadTag load={d.load} name={d.name} plain />
              ) : (
                <span className="ca-hovercard-meta">{d.online ? 'No reading yet' : 'Away'}</span>
              )}
            </div>
          ))}
          {calmOne && <p className="ca-hovercard-meta">The quick panel offers a calm one that has the repo you start in: {chordSigns(CALMER_CHORD)} there.</p>}
        </div>
      )}
      <p className="ca-hovercard-meta">Read every half a minute: the CPUs over the last five minutes, memory as it is then.</p>
    </HoverCard>
  );
}
