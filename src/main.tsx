import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { watchCaret } from './caret';
import { drawNoticesHere } from './notices';
import { Bar } from './ui/Bar';
import { DesktopLines } from './ui/DesktopLines';
import { DesktopPin } from './ui/DesktopPin';
import { Peek } from './ui/Peek';
import { Quick } from './ui/Quick';
import './styles/app.css';

declare global {
  interface Window {
    /** Set on the floating bar's window before the page loads. */
    __CODEALONG_BAR__?: boolean;
    /** Set on the quick panel's window before the page loads. */
    __CODEALONG_QUICK__?: boolean;
    /** Set on the floating bar's peek before the page loads. */
    __CODEALONG_PEEK__?: boolean;
    /** Set on the window of a session kept on the desktop before the page loads. */
    __CODEALONG_KEPT__?: boolean;
    /** Set on the windows of cards pinned to the desktop before the page loads. */
    __CODEALONG_DESKTOP__?: boolean;
    /** Set on the window of lines between cards pinned to the desktop before the page loads. */
    __CODEALONG_LINEAGE__?: boolean;
  }
}

// Every window loads this page; the floating bar's shows the bar, its peek's the peek, a session
// kept on the desktop's that session as the peek shows it, the quick panel's the panel, a card
// pinned to the desktop's the card, and the window under the cards the lines between them,
// instead of the board.
const isBar = window.__CODEALONG_BAR__ === true;
const isQuick = window.__CODEALONG_QUICK__ === true;
const isKept = window.__CODEALONG_KEPT__ === true;
const isPeek = window.__CODEALONG_PEEK__ === true || isKept;
const isDesktop = window.__CODEALONG_DESKTOP__ === true;
const isLineage = window.__CODEALONG_LINEAGE__ === true;
if (isBar) document.documentElement.classList.add('is-bar');
if (isQuick) document.documentElement.classList.add('is-quick');
if (isPeek) document.documentElement.classList.add('is-peek');
if (isDesktop) document.documentElement.classList.add('is-desktop');
if (isLineage) document.documentElement.classList.add('is-lineage');
// The board's windows draw the notices, at the end of the accounts bar: their own, and those the
// other windows hand on.
if (!isBar && !isQuick && !isPeek && !isDesktop && !isLineage) drawNoticesHere();

createRoot(document.getElementById('root')!).render(<StrictMode>{isBar ? <Bar /> : isQuick ? <Quick /> : isPeek ? <Peek kept={isKept} /> : isDesktop ? <DesktopPin /> : isLineage ? <DesktopLines /> : <App />}</StrictMode>);

watchCaret();

// Right-click shows Codealong's own menus; the webview's (Reload, Back…) doesn't belong in an
// app. Text fields keep theirs, for copy and paste.
document.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof Element) || !e.target.closest('input, textarea, [contenteditable="true"]')) e.preventDefault();
});
