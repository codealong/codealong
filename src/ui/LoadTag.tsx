// How busy a machine is, in a word and a colored dot (src/machineLoad.ts): beside each machine in
// the quick panel's machine menu and on its card in Settings › Devices, and in the usage bar while
// this machine is strained, where it says "Machine strained" so it isn't taken for a plan's usage.
// Its numbers are in the tooltip.

import { LOAD_WORDS, loadSentence } from '../machineLoad';
import type { Load } from '../types';
import { cx, Icon } from './primitives';

export function LoadTag({ load, name, bar }: { load: Load; name: string; bar?: boolean }) {
  return (
    <span className={cx('ca-load', `is-${load.level}`, bar && 'is-bar')} title={loadSentence(load, name)}>
      {bar ? <Icon name="gauge" size={13} /> : <i className="ca-load-dot" aria-hidden="true" />}
      {bar ? `Machine ${load.level}` : LOAD_WORDS[load.level]}
    </span>
  );
}
