// What an automation's agent is told, and which agent: a box drawn as a session's box is
// (SessionReply.tsx), its text the instructions and, at its foot, the agent menu (AgentMenu.tsx)
// with Auto and the routing groups over each account with its models and effort.
// Settings › Automation's reviewer is one (SettingsDialog.tsx `ReviewsField`); another automation
// that runs an agent on instructions of yours takes the same box.
//
// The box shows what the agent is told: the instructions you wrote, else the default ones, which
// are saved as no text at all, so a later default goes on applying. What's written is kept as the
// box loses the keyboard; the agent as it's picked.

import { useEffect, useMemo, useRef, useState } from 'react';
import { AUTO, isAuto } from '../start';
import type { Lane, RoutingGroup, StartAccount, Usage } from '../types';
import { AgentMenu } from './AgentMenu';
import { BoxText } from './primitives';

export interface AgentPromptBoxProps {
  /** The accounts it may run on, as start options list them. */
  accounts: StartAccount[];
  /** Routing groups configured in `[routing]`, when any. */
  groups?: RoutingGroup[];
  /** The routing group Auto delegates to (`[routing] auto`), when set. */
  autoGroup?: string;
  /** Recent lanes, so alternate-harness and round-robin groups reflect recent starts. */
  lanes?: Lane[];
  usage: Usage[];
  now: number;
  /** The account or routing group picked, empty for Auto, and its model and effort, empty for the
   *  account's own. */
  account: string;
  model: string;
  effort: string;
  /** The instructions written; empty for `defaultPrompt`. */
  prompt: string;
  defaultPrompt: string;
  /** What the box is, for a screen reader, and what the empty box says. */
  label: string;
  hint: string;
  /** What Auto means here, on its line in the menu. */
  autoTitle: string;
  /** What the chip says Auto or a routing group starts, where that isn't one account for every run
   *  (another harness than each PR's session's); else the account it would start now. */
  autoSays?: string;
  /** What going back to the default instructions says. */
  defaultLabel?: string;
  disabled?: boolean;
  onAgent: (account: string, model: string, effort: string) => void;
  onPrompt: (prompt: string) => void;
}

export function AgentPromptBox({ accounts, groups = [], autoGroup, lanes = [], usage, now, account, model, effort, prompt, defaultPrompt, label, hint, autoTitle, autoSays, defaultLabel = 'Use the default instructions', disabled, onAgent, onPrompt }: AgentPromptBoxProps) {
  const group = useMemo(() => (autoGroup ? (groups.find((g) => g.name === autoGroup) ?? null) : null), [autoGroup, groups]);
  const auto = useMemo(() => ({ title: autoTitle, says: autoSays, groups, group, lanes }), [autoTitle, autoSays, groups, group, lanes]);

  const shown = prompt.trim() || defaultPrompt;
  const [draft, setDraft] = useState(shown);
  // What's saved, or gone back to the default, shows; what's being written stays until it's kept.
  useEffect(() => setDraft(shown), [shown]);
  const keep = () => {
    const written = draft.trim();
    const next = written === defaultPrompt.trim() || written === '' ? '' : written;
    if (next !== prompt.trim()) onPrompt(next);
  };
  const box = useRef<HTMLTextAreaElement>(null);
  return (
    <div className="ca-agentbox">
      <div
        className="ca-composer-field"
        onMouseDown={(e) => {
          if ((e.target as Element).closest('button, textarea, .ca-picker')) return;
          e.preventDefault();
          box.current?.focus();
        }}
      >
        <BoxText ref={box} value={draft} lines={12} hint={hint} aria-label={label} disabled={disabled} spellCheck={false} onChange={(e) => setDraft(e.target.value)} onBlur={keep} />
        <div className="ca-composer-tools">
          {prompt.trim() && (
            <button type="button" className="ca-linkbtn" disabled={disabled} onClick={() => onPrompt('')}>
              {defaultLabel}
            </button>
          )}
        </div>
        <div className="ca-composer-buttons">
          <AgentMenu
            accounts={accounts}
            usage={usage}
            now={now}
            value={{ account: account || AUTO, model: model || null, effort: effort || null }}
            auto={auto}
            title="Which account it runs on, on which model and at which effort"
            disabled={disabled || accounts.length === 0}
            onPick={(p) => (isAuto(p.account) ? onAgent('', '', '') : onAgent(p.account, p.model ?? '', p.effort ?? ''))}
            onDone={() => box.current?.focus()}
            down
          />
        </div>
      </div>
    </div>
  );
}
