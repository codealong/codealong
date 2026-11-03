// Checks a session's terminals of Codealong's own in the main window (ui/terminal/,
// terminals.ts, menus.ts `ownTerminal`): ⌃` from the session's box opens one in the drawer under
// the box, in the session's folder, which takes the keyboard; keys typed there reach it as keys,
// in order, with what they typed, and ⌘ keys stay the app's but copy; a paste with line breaks
// asks before it goes; ⌃` gives the box the keyboard back; the session's menu says Show terminal
// with ⌃`; the drawer folds to its tabs and opens again; it pops out into a window of its own and
// back; and a tab's × closes its terminal.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/terminal-check.js
(function () {
  (async function () {
    try {
      var panel = await until(function () { return q('.ca-sessinsp[data-shows-session]'); }, 30000, 'a session in the panel');
      var sessionKey = panel.getAttribute('data-shows-session');
      var box = await until(function () { return q('.ca-composer-field textarea', panel); }, 10000, 'the session’s box');
      check('no drawer before there’s a terminal', !q('.ca-termdrawer', panel));
      box.focus();
      key(box, { key: '`', code: 'Backquote', ctrlKey: true });
      var opened = await until(function () { return last('terminal_open'); }, 5000, 'a terminal opened');
      check('⌃` opens a terminal for the session, in its folder, a shell', opened.key === sessionKey && !!opened.folder && opened.command === null, JSON.stringify(opened));
      check('at the size and in the colors the window has', !!opened.theme && typeof opened.theme.bg === 'number' && opened.theme.palette.length === 16, JSON.stringify(opened.theme));
      var drawer = await until(function () { return q('.ca-termdrawer.is-open', panel); }, 5000, 'the drawer open');
      var tabs = all('.ca-termtab', drawer);
      check('under the box, with a tab for it', tabs.length === 1 && /zsh/.test(tabs[0].textContent) && drawer.previousElementSibling && !!q('.ca-composer-field', drawer.previousElementSibling.parentElement), tabs.map(function (t) { return t.textContent; }).join(' | '));
      var field = await until(function () { return q('.ca-term-field', drawer); }, 5000, 'the terminal');
      await until(function () { return document.activeElement === field; }, 3000, 'the terminal with the keyboard');
      check('it takes the keyboard', document.activeElement === field);
      var canvas = q('.ca-term-canvas', drawer);
      await until(function () { return canvas.dataset.renderer; }, 3000, 'a renderer');
      note('drawn with ' + canvas.dataset.renderer);
      await until(function () { return sent('terminal_watch').some(function (w) { return w.on; }) && sent('terminal_resize').length; }, 3000, 'the window watching it at its size');
      var size = last('terminal_resize').size;
      check('the window says it shows it, and its size in whole cells', size.cols > 10 && size.rows > 2 && size.cellWidth > 0 && size.cellHeight > 0, JSON.stringify(size));

      // Typing: keys in order, with what they typed; Ctrl by its key alone.
      var before = calls('terminal_input').length;
      [['l', 'KeyL'], ['s', 'KeyS']].forEach(function (k) { key(field, { key: k[0], code: k[1] }); });
      key(field, { key: 'Enter', code: 'Enter' });
      key(field, { key: 'c', code: 'KeyC', ctrlKey: true });
      await until(function () {
        var keys = sent('terminal_input').slice(before).flatMap(function (a) { return a.inputs; }).filter(function (i) { return i.kind === 'key'; });
        return keys.length >= 4 && keys;
      }, 3000, 'the keys sent');
      var keys = sent('terminal_input').slice(before).flatMap(function (a) { return a.inputs; }).filter(function (i) { return i.kind === 'key'; });
      check('keys reach it in order, with what they typed', keys.map(function (i) { return i.code + ':' + (i.text || ''); }).join(' ') === 'KeyL:l KeyS:s Enter: KeyC:', keys.map(function (i) { return i.code + ':' + (i.text || ''); }).join(' '));
      check('Ctrl goes to the program', keys[3].mods.ctrl === true);

      // ⌘C copies the selection; another ⌘ key isn't the terminal's.
      var inputsNow = calls('terminal_input').length;
      key(field, { key: 'c', code: 'KeyC', metaKey: true });
      await until(function () { return calls('terminal_copy').length; }, 3000, 'copy');
      check('⌘C copies what’s selected', calls('terminal_copy').length === 1);
      key(field, { key: 'k', code: 'KeyK', metaKey: true });
      await wait(200);
      check('⌘ keys stay the app’s', calls('terminal_input').slice(inputsNow).every(function (c) { return !c[1].inputs.some(function (i) { return i.mods && i.mods.meta; }); }));

      // A paste with line breaks asks first.
      var paste = new Event('paste', { bubbles: true, cancelable: true });
      paste.clipboardData = { getData: function () { return 'rm -rf build\nnpm run deploy'; } };
      field.dispatchEvent(paste);
      var ask = await until(function () { return q('.ca-term-ask', drawer); }, 3000, 'the paste asked about');
      check('a paste with line breaks asks before it goes', /Paste 2 lines\?/.test(ask.textContent) && sent('terminal_paste').length === 1 && sent('terminal_paste')[0].force === false, ask.textContent);
      all('button', ask).filter(function (b) { return b.textContent.trim() === 'Paste'; })[0].click();
      await until(function () { return sent('terminal_paste').length === 2; }, 3000, 'the paste sent');
      check('and goes once you say so', sent('terminal_paste')[1].force === true && !q('.ca-term-ask', drawer));

      // ⌃` gives the box the keyboard back.
      field.focus();
      key(field, { key: '`', code: 'Backquote', ctrlKey: true });
      await until(function () { return document.activeElement === box; }, 3000, 'the box with the keyboard');
      check('⌃` in the terminal gives the box the keyboard back', document.activeElement === box);

      // The session's menu: Show terminal, with its key.
      var row = await until(function () { return q('.ca-sessionlist .ca-lane.is-selected'); }, 3000, 'the session’s row');
      rightClick(row);
      await until(function () { return menuLabels().length; }, 3000, 'its menu');
      var labels = menuLabels();
      check('its menu shows its terminal', labels.some(function (l) { return /^Show terminal/.test(l); }), labels.join(' | '));

      // Folding and opening the drawer.
      q('.ca-termdrawer-act[aria-label="Fold the terminals"]', drawer).click();
      await until(function () { return !q('.ca-termdrawer.is-open', panel); }, 3000, 'the drawer folded');
      check('folded, it’s its tabs alone', !!q('.ca-termdrawer .ca-termtab', panel) && !q('.ca-term-canvas', panel));
      check('and the window stops watching it', sent('terminal_watch').some(function (w) { return w.on === false; }));
      q('.ca-termtab-name', panel).click();
      await until(function () { return q('.ca-termdrawer.is-open .ca-term-canvas', panel); }, 3000, 'the drawer open again');
      check('a tab opens it again', !!q('.ca-termdrawer.is-open', panel));

      // Popping it out into a window of its own, and putting it back.
      q('.ca-termdrawer-act[aria-label="Pop it out"]', panel).click();
      await until(function () { return calls('terminal_window').length; }, 3000, 'popped out');
      check('it pops out into a window of its own, which takes the keyboard', last('terminal_window').key === sessionKey && last('terminal_window').keys === true);
      var away = await until(function () { return q('.ca-term-away', panel); }, 3000, 'the drawer saying where it is');
      check('the drawer leaves it to its window', /window of its own/.test(away.textContent) && !q('.ca-term-canvas', panel));
      all('button', away).filter(function (b) { return /Put it back/.test(b.textContent); })[0].click();
      await until(function () { return calls('terminal_put_back').length && q('.ca-term-canvas', panel); }, 3000, 'put back');
      check('and takes it back', !q('.ca-term-away', panel) && last('terminal_put_back').id === last('terminal_window').id);

      // Closing it.
      q('.ca-termtab-close', panel).click();
      await until(function () { return calls('terminal_close').length && !q('.ca-termdrawer', panel); }, 5000, 'the terminal closed');
      check('its tab’s × closes it, and the drawer goes with the last one', calls('terminal_close').length === 1 && !q('.ca-termdrawer', panel));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
