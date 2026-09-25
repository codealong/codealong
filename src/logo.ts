import { useEffect } from 'react';
import type { LogoStatus } from './types';

export const LOGO_LABEL: Record<LogoStatus, string> = {
  error: 'Session error',
  needs: 'Needs you',
  unread: 'Your turn',
  working: 'Working',
  idle: 'Idle',
};

/** The browser icon follows the same engine status as the UI and native Dock icon. */
export function useLogo(status: LogoStatus = 'idle'): LogoStatus {
  useEffect(() => {
    const icon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    if (icon) icon.href = `${import.meta.env.BASE_URL}logos/${status}.svg`;
  }, [status]);
  return status;
}
