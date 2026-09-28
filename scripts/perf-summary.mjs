#!/usr/bin/env node
// Sums up ~/Library/Logs/Codealong/perf.log (written while `[debug] perf_log` is on), so a
// change can be judged against the same numbers before and after.
//
//   node scripts/perf-summary.mjs                 the app's latest run
//   node scripts/perf-summary.mjs --since 09:30   the latest run from that time on
//   node scripts/perf-summary.mjs --runs 3        the last three runs, each on its own
//   node scripts/perf-summary.mjs --log <file>    another log

import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const file = opt('--log') ?? join(homedir(), 'Library/Logs/Codealong/perf.log');
const since = opt('--since');
const runCount = Number(opt('--runs') ?? 1);

const lines = readFileSync(file, 'utf8').split('\n').filter(Boolean);
// Each start of the app writes "perf log on"; a run is the lines from one to the next.
const starts = lines.flatMap((l, i) => (l.includes(' perf log on: ') ? [i] : []));
if (starts.length === 0) starts.push(0);
const runs = starts.map((s, i) => lines.slice(s, starts[i + 1] ?? lines.length)).slice(-runCount);

const num = (re, l) => {
  const m = re.exec(l);
  return m ? Number(m[1]) : null;
};
const pct = (xs, q) => {
  if (xs.length === 0) return '-';
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * s.length))];
};
const dist = (xs, unit = 'ms') => (xs.length ? `median ${pct(xs, 0.5)}, p95 ${pct(xs, 0.95)}, max ${pct(xs, 1)} ${unit}` : 'none');
const avg = (xs) => (xs.length ? Math.round(xs.reduce((a, b) => a + b, 0) / xs.length) : 0);
const clock = (l) => l.slice(0, 8);
const seconds = (t) => {
  const [h, m, s] = t.split(':').map(Number);
  return h * 3600 + m * 60 + s;
};
const top = (xs, n) =>
  Object.entries(xs.reduce((m, x) => ((m[x] = (m[x] ?? 0) + 1), m), {}))
    .sort((a, b) => b[1] - a[1])
    .slice(0, n)
    .map(([k, c]) => `${k} (${c})`)
    .join(', ');

for (let run of runs) {
  const head = run[0];
  if (since) run = run.filter((l) => l.slice(0, 5) >= since);
  if (run.length === 0) {
    console.log(`${head.slice(0, 12)}: nothing since ${since}`);
    continue;
  }
  const span = Math.max(1, seconds(clock(run[run.length - 1])) - seconds(clock(run[0]))) / 60;
  const loads = run.map((l) => num(/load=([\d.]+)/, l)).filter((x) => x != null);
  const perMin = (n) => (n / span).toFixed(1);
  console.log(`\n${head.includes('perf log on') ? head.slice(head.indexOf('perf log on')) : 'perf log'}`);
  console.log(`  ${clock(run[0])} to ${clock(run[run.length - 1])} (${span.toFixed(0)} min), load ${Math.min(...loads)} to ${Math.max(...loads)}, median ${pct(loads, 0.5)}`);

  const pubs = run.filter((l) => / publish \d+ KB/.test(l));
  const skipped = run.filter((l) => l.includes('publish skipped')).length;
  console.log(`\n  publishes: ${pubs.length} (${perMin(pubs.length)}/min), ${skipped} skipped as unchanged, ${avg(pubs.map((l) => num(/publish (\d+) KB/, l)))} KB each`);
  console.log(`    compose ${dist(pubs.map((l) => num(/compose (\d+) ms/, l)))}`);
  console.log(`    hash    ${dist(pubs.map((l) => num(/hash (\d+) ms/, l)))}`);
  console.log(`    emit    ${dist(pubs.map((l) => num(/emit (\d+) ms/, l)))}`);
  const changed = pubs.map((l) => num(/changed (\d+) of \d+ lanes/, l));
  console.log(`    lanes changed per publish: ${dist(changed, '')}`);

  const lag = (who) => run.filter((l) => l.includes(` ${who} lag `)).map((l) => num(/lag (\d+) ms/, l));
  console.log(`\n  engine lag (a message waiting for the engine, 50 ms and up): ${lag('engine').length} times, ${dist(lag('engine'))}`);
  console.log(`  main thread lag: ${lag('main thread').length} times, ${dist(lag('main thread'))}`);
  // Samples of a stuck main thread: where it was, from its innermost frame.
  const stuck = run.filter((l) => l.includes(' main thread stuck for '));
  for (const l of stuck.slice(-5)) console.log(`    ${clock(l)} ${l.slice(l.indexOf('main thread stuck for '))}`);
  if (stuck.length > 5) console.log(`    and ${stuck.length - 5} earlier samples`);
  const keys = run.filter((l) => / ui\[[^\]]+\] keys: /.test(l));
  if (keys.length) console.log(`  typed keys, from the key to its frame (worst per second): ${dist(keys.map((l) => num(/, (\d+) ms at most/, l)))}`);

  const git = run.filter((l) => / git \w+ \d+ ms/.test(l));
  console.log(`\n  git runs (slow ones logged): ${git.length} (${perMin(git.length)}/min), ${dist(git.map((l) => num(/git \w+ (\d+) ms/, l)))}`);
  if (git.length) console.log(`    most in: ${top(git.map((l) => l.slice(l.lastIndexOf(': ') + 2)), 3)}`);

  const fs = run.filter((l) => l.includes(' fs batch: '));
  const fsMs = fs.map((l) => num(/paths, (\d+) ms/, l));
  console.log(`\n  slow fs batches: ${fs.length}, ${fsMs.reduce((a, b) => a + b, 0)} ms in all, paths ${dist(fs.map((l) => num(/batch: (\d+) paths/, l)), '')}`);
  const where = fs.flatMap((l) => (l.includes(', most in ') ? [l.slice(l.indexOf(', most in ') + 10).split(' (')[0]] : []));
  if (where.length) console.log(`    big batches mostly in: ${top(where, 3)}`);

  const windows = [...new Set(run.flatMap((l) => (/ ui\[[^\]]+\]/.test(l) ? [/ ui\[([^\]]+)\]/.exec(l)[1]] : [])))];
  for (const w of windows) {
    const mine = run.filter((l) => l.includes(` ui[${w}] `));
    const pushes = mine.filter((l) => l.includes(' push: '));
    const input = mine.filter((l) => l.includes(' input: ')).map((l) => num(/waited up to (\d+) ms/, l));
    const stalls = mine.filter((l) => l.includes(' stall: ')).map((l) => num(/ran (\d+) ms late/, l));
    console.log(`\n  window ${w}: ${pushes.length} pushes, commit ${dist(pushes.map((l) => num(/commit (\d+) ms/, l)))}`);
    if (input.length) console.log(`    pointer waits over 50 ms: ${input.length} seconds, ${dist(input)}`);
    if (stalls.length) console.log(`    stalls: ${stalls.length}, ${dist(stalls)}`);
  }
}
