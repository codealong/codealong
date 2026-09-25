import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { Bar } from './ui/Bar';
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
  }
}

// Every window loads this page; the floating bar's shows the bar and the quick panel's the
// panel instead of the board.
const isBar = window.__CODEALONG_BAR__ === true;
const isQuick = window.__CODEALONG_QUICK__ === true;
if (isBar) document.documentElement.classList.add('is-bar');
if (isQuick) document.documentElement.classList.add('is-quick');

createRoot(document.getElementById('root')!).render(<StrictMode>{isBar ? <Bar /> : isQuick ? <Quick /> : <App />}</StrictMode>);

// Right-click shows Codealong's own menus; the webview's (Reload, Back…) doesn't belong in an
// app. Text fields keep theirs, for copy and paste.
document.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof Element) || !e.target.closest('input, textarea, [contenteditable="true"]')) e.preventDefault();
});
