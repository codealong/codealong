// Due dates on todos: a day on the calendar, written `2026-10-03`, with no time or zone, so it's
// the same day wherever it's read. The engine keeps it (`due` in todos.rs); this file has how
// the windows say it and the days a menu offers. A due date only says when you mean a todo done
// by: the list keeps your order, and nothing changes when the day comes.

const DAY = 24 * 3600e3;

/** The day `at` falls on here, as a due date is written. */
export function dayKey(at: number): string {
  const d = new Date(at);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** A due date as a local Date at midnight, or null when it doesn't read as a day. */
export function dueDate(due: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(due);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return d.getDate() === Number(m[3]) ? d : null;
}

/** Days from today to `due`: 0 today, below 0 once it's past; null when it doesn't read. Counted
 *  on calendar days, so a change to or from daylight saving time doesn't shift it. */
export function daysUntil(due: string, now: number): number | null {
  const d = dueDate(due);
  if (!d) return null;
  const t = new Date(now);
  return Math.round((Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) - Date.UTC(t.getFullYear(), t.getMonth(), t.getDate())) / DAY);
}

/** Overdue once the day has passed, today on the day, soon within the week, later beyond. */
export type DueTone = 'overdue' | 'today' | 'soon' | 'later';

export interface DueSaid {
  /** What a row's chip says: `Due today`, `Due Fri`, `Due Oct 12`, `2 days overdue`. */
  text: string;
  /** The same, as a panel or a menu says it: `Friday, Oct 2`, with how far off it is. */
  long: string;
  tone: DueTone;
}

/** How a due date reads on `now`'s day. */
export function dueWords(due: string, now: number): DueSaid | null {
  const d = dueDate(due);
  const days = daysUntil(due, now);
  if (!d || days === null) return null;
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  const date = d.toLocaleDateString('en-US', sameYear ? { weekday: 'long', month: 'short', day: 'numeric' } : { weekday: 'long', month: 'short', day: 'numeric', year: 'numeric' });
  const short = d.toLocaleDateString('en-US', sameYear ? { month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric', year: 'numeric' });
  if (days < -1) return { text: `${-days} days overdue`, long: `${date}, ${-days} days ago`, tone: 'overdue' };
  if (days === -1) return { text: 'Due yesterday', long: `${date}, yesterday`, tone: 'overdue' };
  if (days === 0) return { text: 'Due today', long: `${date}, today`, tone: 'today' };
  if (days === 1) return { text: 'Due tomorrow', long: `${date}, tomorrow`, tone: 'soon' };
  if (days < 7) return { text: `Due ${d.toLocaleDateString('en-US', { weekday: 'short' })}`, long: `${date}, in ${days} days`, tone: 'soon' };
  return { text: `Due ${short}`, long: `${date}, in ${days} days`, tone: 'later' };
}

export interface DueChoice {
  label: string;
  due: string;
}

/** The days a menu offers: today, tomorrow, the coming Friday and the coming Monday, each once,
 *  in order. A day within the week goes by its name; a week off, `Next Friday`. */
export function dueChoices(now: number): DueChoice[] {
  const today = new Date(now);
  const at = (days: number) => new Date(today.getFullYear(), today.getMonth(), today.getDate() + days);
  const ahead = (weekday: number) => ((weekday - today.getDay() + 7) % 7) || 7;
  const days = [...new Set([0, 1, ahead(5), ahead(1)])].sort((a, b) => a - b);
  return days.map((n) => {
    const d = at(n);
    const name = d.toLocaleDateString('en-US', { weekday: 'long' });
    return { label: n === 0 ? 'Today' : n === 1 ? 'Tomorrow' : n < 7 ? name : `Next ${name}`, due: dayKey(d.getTime()) };
  });
}
