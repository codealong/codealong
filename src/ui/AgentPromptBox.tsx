// What an automation's agent is told, and which agent: a box drawn as a session's box is
// (SessionReply.tsx), its text the instructions and, at its foot, the chip the quick panel starts
// a session with, opening the same menu of Auto and each account with its models
// (StartControls.tsx `agentMenuSections`). Settings › Automation's reviewer is one
// (SettingsDialog.tsx `ReviewsField`); another automation that runs an agent on instructions of
// yours takes the same box.
//
// The box shows what the agent is told: the instructions you wrote, else the default ones, which
// are saved as no text at all, so a later default goes on applying. What's written is kept as the
// box loses the keyboard; the agent as it's picked.

import { useEffect, useMemo, useRef, useState } from 'react';
import { agentChipWords, agentId, agentOptions, AUTO, autoPick, isAuto, parseAgent } from '../start';
import type { StartAccount, Usage } from '../types';
import { BoxMenu } from './ModelMenu';
import { BoxText } from './primitives';
import { agentMenuSections } from './StartControls';

export interface AgentPromptBoxProps {
  /** The accounts it may run on, as start options list them. */
  accounts: StartAccount[];
  usage: Usage[];
  now: number;
  /** The account picked, empty for Auto, and its model, empty for the account's own. */
  account: string;
  model: string;
  /** The instructions written; empty for `defaultPrompt`. */
  prompt: string;
  defaultPrompt: string;
  /** What the box is, for a screen reader, and what the empty box says. */
  label: string;
  hint: string;
  /** What Auto means here, on its line in the menu. */
  autoTitle: string;
  /** What going back to the default instructions says. */
  defaultLabel?: string;
  disabled?: boolean;
  onAgent: (account: string, model: string) => void;
  onPrompt: (prompt: string) => void;
}

export function AgentPromptBox({ accounts, usage, now, account, model, prompt, defaultPrompt, label, hint, autoTitle, defaultLabel = 'Use the default instructions', disabled, onAgent, onPrompt }: AgentPromptBoxProps) {
  const models = useMemo(() => accounts.flatMap((a) => a.models ?? []).filter((m, i, all) => all.findIndex((x) => x.id === m.id) === i), [accounts]);
  const pick = useMemo(() => autoPick(accounts, usage, now), [accounts, usage, now]);
  const agents = useMemo(() => agentOptions(accounts, models, pick), [accounts, models, pick]);
  const chosen = account ? agentId(account, model || null) : AUTO;
  const sections = agentMenuSections(agents, accounts, pick, usage, models, now, chosen, autoTitle);
  const chip = agentChipWords({ auto: !account, account: account || pick?.account || null, accounts: accounts.length, remote: null, model: model || null, effortWord: null });

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
          <BoxMenu
            kind="model"
            sections={sections}
            value={chosen}
            icon={account ? undefined : 'sparkles'}
            label={chip.label}
            detail={chip.detail}
            title="Which account it runs on, and on which model"
            placeholder="Account or model"
            disabled={disabled || accounts.length === 0}
            onPick={(id) => {
              const { choice, model: m } = parseAgent(id);
              if (!choice || isAuto(choice)) onAgent('', '');
              else onAgent(choice, m ?? '');
            }}
            onDone={() => box.current?.focus()}
            down
          />
        </div>
      </div>
    </div>
  );
}
