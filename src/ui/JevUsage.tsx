// Settings › Jev's usage: what Codealong's requests to Jev used today or over the last days, in
// all and by the feature that asked, from the token ledger (ledger.rs `jev_report`). Its table
// reads as the Usage window's does; the sums are jevUsage.ts's.

import { useEffect, useState } from 'react';
import { jevUsage } from '../api';
import { absolute, count } from '../format';
import { countedSince, JEV_SPANS, jevMoney, jevTotals } from '../jevUsage';
import { errorText } from '../report';
import { tokenCount } from '../spend';
import type { JevReport } from '../types';
import { Seg } from './primitives';

/** The days read: the longest period the page sums over. */
const DAYS = Math.max(...JEV_SPANS.map((s) => s.id));

interface Props {
  /** A feature's name on the page, from its switch's name in `[jev]` (or `test`). */
  nameOf: (feature: string) => string;
  /** Changes when the page should read again: after the Test button asked Jev. */
  stamp: number;
  /** Whether there's a key. Without one, and with nothing ever asked, there's nothing to say. */
  hasKey: boolean;
}

export function JevUsage({ nameOf, stamp, hasKey }: Props) {
  const [report, setReport] = useState<JevReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [span, setSpan] = useState(7);

  useEffect(() => {
    let live = true;
    jevUsage(DAYS).then(
      (r) => {
        if (!live) return;
        setReport(r);
        setError(null);
      },
      (e: unknown) => {
        if (live) setError(errorText(e));
      },
    );
    return () => {
      live = false;
    };
  }, [stamp]);

  if (!hasKey && report?.since == null) return null;
  const { total, features } = report ? jevTotals(report, span) : { total: null, features: [] };
  const since = report ? countedSince(report, span) : null;
  const top = Math.max(1e-12, ...features.map((f) => f.used.cost));

  return (
    <div className="ca-field">
      <span id="ca-jev-usage-label">Usage</span>
      <Seg className="ca-settings-theme" labelledBy="ca-jev-usage-label" options={JEV_SPANS} value={span} onPick={setSpan} />
      {error ? (
        <p className="ca-form-error">{error}</p>
      ) : !report || !total ? (
        <p className="ca-empty">Reading the ledger…</p>
      ) : (
        <>
          <div className="ca-jev-usage-sum">
            <span className="ca-jev-usage-cost">{jevMoney(total.cost)}</span>
            <span className="ca-jev-usage-counts">
              {count(total.requests, 'request')} · {tokenCount(total.tokens)} tokens
            </span>
          </div>
          {features.length > 0 && (
            <table className="ca-usage-table ca-jev-usage">
              <thead>
                <tr>
                  <th scope="col">Feature</th>
                  <th scope="col" className="is-num">
                    Requests
                  </th>
                  <th scope="col" className="is-num">
                    Tokens
                  </th>
                  <th scope="col" className="ca-usage-share-col" aria-label="Against the costliest" />
                  <th scope="col" className="is-num">
                    Cost
                  </th>
                </tr>
              </thead>
              <tbody>
                {features.map(({ feature, used }) => (
                  <tr key={feature}>
                    <th scope="row">{nameOf(feature)}</th>
                    <td className="is-num">{used.requests.toLocaleString('en-US')}</td>
                    <td className="is-num" title={used.requests > 0 ? `${tokenCount(used.tokens / used.requests)} a request` : undefined}>
                      {tokenCount(used.tokens)}
                    </td>
                    <td className="ca-usage-share-col">{used.cost > 0 && <span className="ca-usage-share" style={{ width: `${(used.cost / top) * 100}%` }} />}</td>
                    <td className="is-num">{jevMoney(used.cost)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
      {since != null && <small>Counted since {absolute(since)}, when Codealong began keeping them.</small>}
    </div>
  );
}
