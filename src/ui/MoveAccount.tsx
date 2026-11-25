// Moving or handing off a session to another account or provider, asked first: the chip at the
// right of its box's foot lists every eligible account under its own (SessionAgent.tsx), and
// picking another opens `MoveDialog`, as Move… does in the box a usage limit, cut-off turn or API
// error puts under its header (SessionHeader.tsx). SessionPanel.tsx holds the dialog, so every
// window showing a session asks the same. Same-harness Claude Code default moves take the lossless
// native fast path; cross-provider moves, non-default options, or keeping the original session open
// use the generic handoff engine (handoff.rs, actions.rs `move_session`).

import { useState } from 'react';
import type { MoveOptions } from '../api';
import { shortOf } from '../devices';
import { isNativeMove, moveDialogTitle, moveDialogWords } from '../handoff';
import { HARNESS_WORD } from '../harness';
import type { AccountMoves } from '../menus';
import { cmd } from '../platform';
import { modelName, placeTakesModel } from '../start';
import { useDeviceStartAccounts } from '../startAccounts';
import type { Session, StartPlace } from '../types';
import { Button, CheckBox, Dialog, Icon, Seg } from './primitives';

/** What a native move does, kept for callers that describe one directly. */
export function moveWords(s: Pick<Session, 'title' | 'status' | 'attachCommand'>, to: string, terminal: string, goesOn: boolean): string {
  const where = s.attachCommand != null ? 'as a background job' : `in a new ${terminal} tab`;
  const first = s.status === 'working' ? ' Its turn is stopped first.' : '';
  const then = goesOn ? ', told to carry on where it stopped' : '';
  return `A copy of “${s.title}” goes on under ${to} ${where}${then}, and this one is closed and archived.${first}`;
}

interface MoveDialogProps {
  session: Session;
  moves: AccountMoves;
  /** The account picked to move or hand off to; the one with room, or the first, without one. */
  to: string | null;
  terminal: string;
  onClose: () => void;
}

/** Asks before moving or handing off a session to another account or provider. */
export function MoveDialog({ session: s, moves, to: asked, terminal, onClose }: MoveDialogProps) {
  const first = asked && moves.to.includes(asked) ? asked : (moves.suggested ?? moves.to[0]);
  const [to, setTo] = useState(first);
  const [keep, setKeep] = useState(false);
  const [place, setPlace] = useState<StartPlace | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const [effort, setEffort] = useState<string | null>(null);
  const [prompt, setPrompt] = useState('');

  const startAccounts = useDeviceStartAccounts(shortOf(s.key), true);
  const target = moves.targets.find((t) => t.account === to) ?? moves.targets[0];
  const accountCfg = target ? startAccounts.find((a) => a.name === target.account && a.harness === target.harness) : undefined;
  const places = accountCfg?.places ?? [];
  const effectivePlace: StartPlace | null = place && places.some((p) => p.place === place) ? place : (accountCfg?.chosen ?? places[0]?.place ?? null);
  const takesModelHere = effectivePlace ? placeTakesModel(effectivePlace, false) : true;
  const models = takesModelHere ? (accountCfg?.models ?? []) : [];
  const efforts = takesModelHere ? (accountCfg?.efforts ?? []) : [];
  const isGeneric = !target?.native || keep;

  const trimmedPrompt = prompt.trim();
  const displayOpts: MoveOptions | undefined = isGeneric
    ? {
        keep: keep || undefined,
        place: effectivePlace ?? undefined,
        model: takesModelHere && model ? model : undefined,
        effort: takesModelHere && effort ? effort : undefined,
        prompt: trimmedPrompt || undefined,
      }
    : undefined;

  const pickAccount = (next: string) => {
    setTo(next);
    setPlace(null);
    setModel(null);
    setEffort(null);
  };

  const move = () => {
    const rawOpts: MoveOptions | undefined = isGeneric
      ? {
          keep: keep || undefined,
          place: place ?? undefined,
          model: takesModelHere && model ? model : undefined,
          effort: takesModelHere && effort ? effort : undefined,
          prompt: trimmedPrompt || undefined,
        }
      : undefined;
    const opts = target && isNativeMove(target, rawOpts) ? undefined : rawOpts;
    moves.run(to, opts);
    onClose();
  };

  const title = moveDialogTitle(target, s.harness, keep);
  const lede = target ? moveDialogWords(s, target, terminal, moves.goesOn, displayOpts) : '';
  const hasBody = moves.targets.length > 1 || isGeneric;

  return (
    <Dialog
      className="ca-movedlg"
      title={title}
      lede={lede}
      onClose={onClose}
      onSubmit={move}
      aside={
        <label className="ca-check ca-movedlg-keep" title="Starts a linked handoff copy without closing or archiving this session">
          <CheckBox checked={keep} onChange={setKeep} />
          <span>Keep original open</span>
        </label>
      }
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" autoFocus>
            {keep ? `Hand off to ${to}` : `Move to ${to}`}
          </Button>
        </>
      }
    >
      {hasBody && (
        <div className="ca-dialog-body ca-movedlg-body">
          {moves.targets.length > 1 && (
            <div className="ca-movedlg-to">
              <span className="ca-movedlg-label">To</span>
              <Seg
                label="Move to"
                options={moves.targets.map((t) => ({
                  id: t.account,
                  label: t.account,
                  note:
                    t.account === moves.suggested
                      ? t.harness !== s.harness
                        ? `${HARNESS_WORD[t.harness]} · has room`
                        : 'has room'
                      : t.harness !== s.harness
                        ? HARNESS_WORD[t.harness]
                        : undefined,
                }))}
                value={to}
                onPick={pickAccount}
              />
            </div>
          )}
          {isGeneric && places.length > 1 && (
            <div className="ca-movedlg-row">
              <span className="ca-movedlg-label">Opens in</span>
              <Seg label="Opens in" options={places.map((p) => ({ id: p.place, label: p.label }))} value={effectivePlace} onPick={(p) => setPlace(p)} />
            </div>
          )}
          {isGeneric && models.length > 0 && (
            <label className="ca-field">
              <span>Model</span>
              <span className="ca-start-select">
                <select value={model ?? ''} onChange={(e) => setModel(e.target.value || null)}>
                  <option value="">{accountCfg?.defaultModel ? `Default (${modelName(accountCfg.defaultModel, models)})` : 'Account default'}</option>
                  {models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                </select>
                <Icon name="chevron-down" size={14} />
              </span>
            </label>
          )}
          {isGeneric && efforts.length > 0 && (
            <div className="ca-movedlg-row">
              <span className="ca-movedlg-label">Effort</span>
              <Seg label="Effort" options={[{ id: null as string | null, label: 'Default' }, ...efforts.map((e) => ({ id: e.id as string | null, label: e.label }))]} value={effort} onPick={setEffort} />
            </div>
          )}
          {isGeneric && (
            <label className="ca-field ca-movedlg-note">
              <span>Next step (optional)</span>
              <textarea
                value={prompt}
                rows={2}
                placeholder="Tell the new agent what to focus on next…"
                onChange={(e) => setPrompt(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && cmd(e)) {
                    e.preventDefault();
                    move();
                  }
                }}
              />
            </label>
          )}
        </div>
      )}
    </Dialog>
  );
}
