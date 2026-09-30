import { useEffect, useState } from 'react';

/** The time, again every `everyMs`, for what a window says in words ("5 minutes ago"). */
export function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}
