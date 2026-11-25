// The Analytics window's Experiments tab (`UsageDialog.tsx` shows it once `[experimental]
// experiments` is on): read-only leaderboard and run details from `experiments/report.rs`,
// priced on demand from the token ledger. Adopting and discarding stay in `codealong exp`.

import { useMemo, useState } from 'react';
import { filterLeaderboard, filterRuns, formatDuration, recordLabel, runTotalCost, stageLabel, trialFlags } from '../experiments';
import { agoPhrase } from '../format';
import { money } from '../spend';
import type { ExperimentReport, ExperimentRunView, Session } from '../types';
import { cx, Icon } from './primitives';

interface Props {
  report: ExperimentReport;
  sessions: Map<string, Session>;
  now: number;
  onShowSession: (key: string) => void;
}

export function ExperimentsPart({ report, sessions, now, onShowSession }: Props) {
  const [selected, setSelected] = useState<string>('');
  const [expanded, setExpanded] = useState<string | null>(null);

  const activeFilter = selected && report.experiments.includes(selected) ? selected : '';
  const rows = useMemo(() => filterLeaderboard(report.leaderboard, activeFilter), [report.leaderboard, activeFilter]);
  const runs = useMemo(() => filterRuns(report.runs, activeFilter), [report.runs, activeFilter]);

  if (report.runs.length === 0) {
    return (
      <div className="ca-exp-empty">
        <p className="ca-empty">No experiment runs yet.</p>
        <p className="ca-exp-hint">
          Start one with <code className="ca-ic">codealong exp run --arm claude:opus:high --arm codex:sol-6.2:high --judge antigravity --prompt &quot;…&quot;</code>
        </p>
      </div>
    );
  }

  return (
    <div className="ca-exp">
      {report.experiments.length > 1 && (
        <div className="ca-exp-bar">
          <span className="ca-usage-by-label">Experiment</span>
          <div className="ca-start-select ca-usage-account">
            <select aria-label="Experiment" value={activeFilter} onChange={(e) => setSelected(e.target.value)}>
              <option value="">All experiments</option>
              {report.experiments.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <Icon name="chevron-down" size={12} />
          </div>
        </div>
      )}

      {rows.length > 0 && (
        <section className="ca-exp-section">
          <h3 className="ca-usage-heading">Leaderboard</h3>
          <table className="ca-usage-table ca-exp-table">
            <thead>
              <tr>
                <th>Arm</th>
                <th>Reviewer</th>
                <th className="ca-num">Runs</th>
                <th className="ca-num">W–L–T</th>
                <th className="ca-num">Gate</th>
                <th className="ca-num">Avg Cost</th>
                <th className="ca-num">Avg Time</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const split = r.avgReviewerCostApi > 0 ? `Worker ${money(r.avgWorkerCostApi)} · Reviewer ${money(r.avgReviewerCostApi)}` : `Worker ${money(r.avgWorkerCostApi)}`;
                return (
                  <tr key={r.key}>
                    <td>
                      <div className="ca-exp-arm">
                        <span className="ca-exp-model">
                          {r.workerHarness}/{r.workerModel}
                          {r.workerEffort ? `/${r.workerEffort}` : ''}
                        </span>
                        {!activeFilter && report.experiments.length > 1 && <span className="ca-exp-cohort">{r.experiment}</span>}
                      </div>
                    </td>
                    <td className="ca-exp-muted">{r.reviewer ?? 'Self'}</td>
                    <td className="ca-num">{r.runs}</td>
                    <td className="ca-num">{recordLabel(r)}</td>
                    <td className="ca-num">{r.gateTotal > 0 ? `${r.gatePassed}/${r.gateTotal}` : '—'}</td>
                    <td className="ca-num" title={split}>
                      {money(r.avgCostApi)}
                    </td>
                    <td className="ca-num">{formatDuration(r.avgDurationMs)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </section>
      )}

      <section className="ca-exp-section">
        <h3 className="ca-usage-heading">Runs</h3>
        <div className="ca-exp-runs">
          {runs.map((run) => (
            <RunCard
              key={run.id}
              run={run}
              open={expanded === run.id || runs.length === 1}
              onToggle={() => setExpanded((cur) => (cur === run.id ? null : run.id))}
              sessions={sessions}
              now={now}
              onShowSession={onShowSession}
            />
          ))}
        </div>
      </section>

      <div className="ca-usage-notes">
        <p>
          Adopt or discard trials from the command line: <code className="ca-ic">codealong exp adopt &lt;run&gt;/&lt;trial&gt;</code> or <code className="ca-ic">codealong exp discard &lt;run&gt;</code>. Session costs come from Codealong’s token ledger and may lag by up to a minute.
        </p>
      </div>
    </div>
  );
}

function RunCard({
  run,
  open,
  onToggle,
  sessions,
  now,
  onShowSession,
}: {
  run: ExperimentRunView;
  open: boolean;
  onToggle: () => void;
  sessions: Map<string, Session>;
  now: number;
  onShowSession: (key: string) => void;
}) {
  const totalCost = runTotalCost(run);
  return (
    <div className={cx('ca-exp-run', open && 'is-open')}>
      <button type="button" className="ca-exp-run-head" onClick={onToggle}>
        <span className="ca-exp-run-id">{run.id}</span>
        <span className="ca-exp-run-title">{run.prompt.split('\n')[0]}</span>
        <span className={cx('ca-exp-pill', `is-${run.stage}`)}>{run.stage}</span>
        <span className="ca-exp-run-meta">
          {run.repo}@{run.baseSha.slice(0, 7)} · {money(totalCost)} · {agoPhrase(run.createdAt, now)}
        </span>
      </button>
      {open && (
        <div className="ca-exp-run-body">
          {run.recommendationNote && <p className="ca-exp-rec">{run.recommendationNote}</p>}
          {run.error && (
            <p className="ca-form-error" role="alert">
              {run.error}
            </p>
          )}
          <table className="ca-usage-table ca-exp-table">
            <thead>
              <tr>
                <th>Trial</th>
                <th>Arm</th>
                <th>Stage</th>
                <th>Review</th>
                <th>Gate</th>
                <th className="ca-num">Cost</th>
                <th className="ca-num">Time</th>
                <th>Notes</th>
              </tr>
            </thead>
            <tbody>
              {run.trials.map((t) => {
                const flags = trialFlags(t);
                return (
                  <tr key={t.id} className={cx(t.adopted && 'is-adopted', t.recommended && 'is-recommended')}>
                    <td>{t.id}</td>
                    <td>
                      <div className="ca-exp-arm">
                        {t.workerKey && sessions.has(t.workerKey) ? (
                          <button type="button" className="ca-linkbtn" onClick={() => onShowSession(t.workerKey!)}>
                            {t.label}
                          </button>
                        ) : (
                          <span>{t.label}</span>
                        )}
                        {t.reviewerKey && sessions.has(t.reviewerKey) && (
                          <button type="button" className="ca-linkbtn ca-exp-sublink" onClick={() => onShowSession(t.reviewerKey!)}>
                            review
                          </button>
                        )}
                      </div>
                    </td>
                    <td>
                      <span className={cx('ca-exp-pill', `is-${t.stage}`)}>{stageLabel(t.stage)}</span>
                    </td>
                    <td className="ca-exp-muted">{t.reviewVerdict ?? '—'}</td>
                    <td>{t.gatePassed === true ? 'Pass' : t.gatePassed === false ? 'Fail' : '—'}</td>
                    <td className="ca-num" title={`Worker ${money(t.cost.workerApi)}${t.cost.reviewerApi > 0 ? ` · Reviewer ${money(t.cost.reviewerApi)}` : ''}`}>
                      {money(t.cost.totalApi)}
                    </td>
                    <td className="ca-num">{formatDuration(t.durationMs)}</td>
                    <td>
                      <div className="ca-exp-flags">
                        {flags.map((f) => (
                          <span key={f} className="ca-exp-flag">
                            {f}
                          </span>
                        ))}
                        {t.error && <span className="ca-exp-err">{t.error}</span>}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {run.comparisons.length > 0 && (
            <div className="ca-exp-pairs">
              {run.comparisons.map((c) => (
                <div key={`${c.trialA}-${c.trialB}`} className="ca-exp-pair">
                  <span className="ca-exp-pair-names">
                    {c.labelA} vs {c.labelB}
                  </span>
                  <span className="ca-exp-pair-res">
                    {c.consistentWinner ? `Winner: ${c.consistentWinner}` : 'Inconsistent / tie'}
                    {c.rationale ? ` — ${c.rationale}` : ''}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
