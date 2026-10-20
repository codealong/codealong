// Checks Edit from here (SessionThread.tsx, EditFrom.tsx, menus.ts `edit`) in Sessions' panel: a
// right-click on a prompt of yours with the agent's reply before it offers Edit from here…, the
// session's first prompt doesn't, and the item opens a dialog holding the prompt to rewrite. An
// empty one can't be sent; ⌘⏎ asks for a fork from before that prompt (`fork_session` with the
// prompt's cursor as `from` and your edit as the prompt) and closes the dialog; Escape asks for
// nothing. The conversation is a stand-in of two prompts for the first Claude Code session.
//   scripts/preview.sh sessions '' scripts/preview/edit-from-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to show'); return; }
  var sessionKey = picked.key;
  var at = picked.lastActivity || Date.now();
  var said = function (kind, text, cursor) { return { at: at - (500 - cursor) * 1000, kind: kind, text: text, failed: false, cursor: cursor, n: 0 }; };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'session_page' && args && args.key === sessionKey) {
      return Promise.resolve({ before: null, after: 500, items: [said('you', 'Fix the parser', 100), said('agent', 'Fixed: it reads nested lists now.', 200), said('you', 'Now test it\nwith the old fixtures too', 300), said('agent', 'All 40 pass.', 400)] });
    }
    if (cmd === 'fork_session') {
      window.__calls.push([cmd, args]);
      return Promise.resolve({ key: null, message: 'Forked from before that message: a copy of “x” goes on from there with your edit.' });
    }
    return real.apply(this, arguments);
  };
  var prompts = function () { return all('.ca-thread-said.is-you'); };
  var dialog = function () { return q('.ca-editdlg'); };
  var box = function () { return q('.ca-editdlg textarea'); };
  var send = function () { return q('.ca-editdlg button[type="submit"]'); };

  (async function () {
    try {
      say('edit from here check: opening a conversation');
      await until(function () { return q('[id$="::' + sessionKey + '"]'); }, 20000, 'the session isn’t listed');
      q('[id$="::' + sessionKey + '"]').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await until(function () { return prompts().length === 2; }, 20000, 'no conversation of two prompts');
      var first = await menuOf(prompts()[0]);
      check('the first prompt, with nothing before it, has no Edit from here', first.indexOf('Edit from here…') < 0, first.join(' | '));
      var second = await menuOf(prompts()[1]);
      check('a later prompt has Edit from here…', second.indexOf('Edit from here…') >= 0, second.join(' | '));
      check('after Copy message', second.indexOf('Copy message') === 0, second.join(' | '));
      menuItem('Edit from here…')();
      await until(dialog, 3000, 'no dialog');
      check('it opens a dialog to edit it in', q('.ca-editdlg h2').textContent === 'Edit from here');
      check('which says the session and the files stay', /stays as it is, and so do the files/.test(q('.ca-editdlg .ca-dialog-lede').textContent), q('.ca-editdlg .ca-dialog-lede').textContent);
      check('holding the prompt as you wrote it', box().value === 'Now test it\nwith the old fixtures too', JSON.stringify(box().value));
      check('with the cursor in it, at the end', document.activeElement === box() && box().selectionStart === box().value.length, document.activeElement && document.activeElement.tagName);
      type(box(), '  ');
      await wait(50);
      check('an empty edit can’t be sent', send().disabled);
      key(box(), { key: 'Enter', metaKey: true });
      await wait(50);
      check('nor by ⌘⏎', calls('fork_session').length === 0 && !!dialog());
      type(box(), 'Now lint it');
      await wait(50);
      key(box(), { key: 'Enter', metaKey: true });
      await until(function () { return calls('fork_session').length === 1; }, 3000, 'no fork asked for');
      var asked = calls('fork_session')[0][1];
      check('⌘⏎ forks from before that prompt with the edit', asked.key === sessionKey && asked.from === 300 && asked.prompt === 'Now lint it', JSON.stringify(asked));
      await until(function () { return !dialog(); }, 3000, 'the dialog stayed');
      check('and closes the dialog', true);
      await menuOf(prompts()[1]);
      menuItem('Edit from here…')();
      await until(dialog, 3000, 'no dialog the second time');
      key(document.activeElement, { key: 'Escape' });
      await until(function () { return !dialog(); }, 3000, 'Escape left the dialog up');
      check('Escape closes it without a fork', calls('fork_session').length === 1);
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
