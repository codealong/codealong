import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { Bar } from './ui/Bar';
import { Peek } from './ui/Peek';
import { Quick } from './ui/Quick';
import './styles/tokens.css';
import './styles/components.css';
import './styles/app.css';

declare global {
  interface Window {
    /** Set on the floating bar's window before the page loads. */
    __CODEALONG_BAR__?: boolean;
    /** Set on the quick panel's window before the page loads. */
    __CODEALONG_QUICK__?: boolean;
    /** Set on the floating bar's peek before the page loads. */
    __CODEALONG_PEEK__?: boolean;
  }
}

// Every window loads this page; the floating bar's shows the bar, its peek's the peek, and the
// quick panel's the panel instead of the board.
const isBar = window.__CODEALONG_BAR__ === true;
const isQuick = window.__CODEALONG_QUICK__ === true;
const isPeek = window.__CODEALONG_PEEK__ === true;
if (isBar) document.documentElement.classList.add('is-bar');
if (isQuick) document.documentElement.classList.add('is-quick');
if (isPeek) document.documentElement.classList.add('is-peek');

createRoot(document.getElementById('root')!).render(<StrictMode>{isBar ? <Bar /> : isQuick ? <Quick /> : isPeek ? <Peek /> : <App />}</StrictMode>);

// Right-click shows Codealong's own menus; the webview's (Reload, Back…) doesn't belong in an
// app. Text fields keep theirs, for copy and paste.
document.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof Element) || !e.target.closest('input, textarea, [contenteditable="true"]')) e.preventDefault();
});
