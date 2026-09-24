import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles/tokens.css';
import './styles/components.css';
import './styles/app.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// Right-click shows Codealong's own menus; the webview's (Reload, Back…) doesn't belong in an
// app. Text fields keep theirs, for copy and paste.
document.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof Element) || !e.target.closest('input, textarea, [contenteditable="true"]')) e.preventDefault();
});
