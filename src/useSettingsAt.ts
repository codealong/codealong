// Where the main window's Settings is open (ui/SettingsDialog.tsx, App.tsx): at a section, and at
// one account when a usage chip opened it. ⌘, and the app menu's Settings… come back to the
// section you left, as macOS settings do.

import { useCallback, useEffect, useRef, useState } from 'react';
import { onSettings } from './api';
import type { SettingsSection } from './ui/SettingsDialog';

export function useSettingsAt() {
  const [settings, setSettings] = useState<{ section: SettingsSection; focus: string | null } | null>(null);
  const lastSection = useRef<SettingsSection>('general');
  const openSettings = useCallback((section?: SettingsSection, focus: string | null = null) => {
    setSettings({ section: section ?? lastSection.current, focus });
  }, []);
  const closeSettings = useCallback(() => setSettings(null), []);
  useEffect(() => {
    if (settings) lastSection.current = settings.section;
  }, [settings]);
  useEffect(() => onSettings(() => openSettings()), [openSettings]);
  return { settings, setSettings, openSettings, closeSettings };
}
