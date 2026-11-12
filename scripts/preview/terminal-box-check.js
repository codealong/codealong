// Checks what a terminal beside a session gives the session, how a drag selects past its edge,
// and the edges of its window when it's popped out (ui/terminal/TerminalView.tsx, terminals.ts
// `addToBox`, TerminalWindow.tsx, useSizeGrips.tsx). In a session's drawer: a drag that goes
// past the terminal's top is sent with where it went, which the engine scrolls on from
// (screen.rs `autoscroll`); its right-click menu sends the selection to the session as a code
// block, leaving the box as it was, adds it to the session's box, the box taking the keyboard,
// and all its text, longer than a box takes, as a file attached. Popped out: the menu has the
// same, and its right edge, bottom edge and the corner between them size its window
// (kept_resize), clear of the terminal itself.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/terminal-box-check.js
//   PREVIEW_WAIT=30000 scripts/preview.sh terminal '' scripts/preview/terminal-box-check.js
(function () {
  var pointer = function (el, type, x, y, buttons) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: buttons, screenX: x, screenY: y, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };
  // setPointerCapture needs a real pointer: the stand-in keeps the drag going.
  Element.prototype.setPointerCapture = function () {};
  var SEND = 'Send to the session';
  var ADD = 'Add to the session’s box';
  var ADD_ALL = 'Add all text to the session’s box';

  async function drawer() {
    var panel = await until(function () { return q('.ca-sessinsp[data-shows-session]'); }, 30000, 'a session in the panel');
    var box = await until(function () { return q('.ca-composer-field textarea', panel); }, 10000, 'the session’s box');
    box.focus();
    key(box, { key: '`', code: 'Backquote', ctrlKey: true });
    var drawn = await until(function () { return q('.ca-termdrawer.is-open .ca-term-canvas', panel); }, 5000, 'a terminal under the box');
    await until(function () { return drawn.dataset.renderer; }, 3000, 'a renderer');
    var canvas = drawn;

    // A drag that goes on above the terminal's top: the move is sent from there, past the edge.
    var r = canvas.getBoundingClientRect();
    pointer(canvas, 'pointerdown', r.left + 20, r.top + 20, 1);
    pointer(canvas, 'pointermove', r.left + 20, r.top - 30, 1);
    pointer(canvas, 'pointerup', r.left + 20, r.top - 30, 0);
    await until(function () { return sent('terminal_input').some(function (s) { return s.inputs.some(function (i) { return i.kind === 'mouse' && i.action === 'up'; }); }); }, 3000, 'the drag sent');
    var moves = [].concat.apply([], sent('terminal_input').map(function (s) { return s.inputs; })).filter(function (i) { return i.kind === 'mouse' && i.action === 'move'; });
    check('a drag past the top is sent from where it went, above the screen', moves.some(function (m) { return m.y < 0; }), JSON.stringify(moves));

    var labels = await menuOf(canvas);
    check('its menu sends the selection to the session, or adds it or everything to the session’s box', labels.indexOf(SEND) >= 0 && labels.indexOf(ADD) >= 0 && labels.indexOf(ADD_ALL) >= 0, labels.join(' | '));
    box.value = '';
    box.blur();
    menuItem(SEND)();
    var send = await until(function () { return calls('send_to_session')[0]; }, 3000, 'the selection sent');
    var shownKey = panel.getAttribute('data-shows-session');
    check('the selection goes to the session shown, as a code block', send[1].key === shownKey && send[1].text === '```\nnpm test\n```', JSON.stringify(send[1]));
    var note = await until(function () { var n = q('.ca-term-added', panel); return n && /Sent/.test(n.textContent) ? n : null; }, 3000, 'a note that it went');
    check('the terminal says it went', /Sent the selection to the session/.test(note.textContent), note.textContent);
    check('and the box is left as it was', box.value === '' && document.activeElement !== box, JSON.stringify(box.value));

    // One that doesn't go is in the box of the window it was sent from, as a box's own send is.
    window.__failSend = 'Its terminal is gone.';
    await menuOf(canvas);
    menuItem(SEND)();
    var failed = await until(function () { var n = q('.ca-term-added.is-error', panel); return n; }, 3000, 'a note that it didn’t go');
    check('a send that doesn’t go says why and where it is', /Its terminal is gone\. It’s in the session’s box\./.test(failed.textContent), failed.textContent);
    await until(function () { return /```\nnpm test\n```/.test(box.value); }, 3000, 'the unsent selection in the box');
    check('and it’s in this window’s box', box.value === '```\nnpm test\n```', JSON.stringify(box.value));
    window.__failSend = null;
    box.value = '';
    box.dispatchEvent(new Event('input', { bubbles: true }));

    menuItem(ADD)();
    await until(function () { return /```\nnpm test\n```/.test(box.value); }, 3000, 'the selection in the box');
    check('the selection goes into the box as a code block', /```\nnpm test\n```$/.test(box.value), JSON.stringify(box.value));
    await until(function () { return document.activeElement === box; }, 3000, 'the box with the keyboard');
    check('and the box takes the keyboard, to say what it’s for', document.activeElement === box);
    var added = await until(function () { return q('.ca-term-added', panel); }, 3000, 'a note of it');
    check('the terminal says where it went', /session’s box/.test(added.textContent), added.textContent);

    window.__terminalText = Array.from({ length: 60 }, function (_, i) { return 'log line ' + i; }).join('\n');
    await menuOf(canvas);
    menuItem(ADD_ALL)();
    var saved = await until(function () { return calls('save_attachment')[0]; }, 3000, 'a file kept');
    check('all of a long one goes as a file of its own', decodeURIComponent(saved[2].headers['x-name']) === 'terminal.txt', JSON.stringify(saved[2]));
    var strip = await until(function () { var s = q('.ca-imgstrip', panel); return s && /terminal\.txt/.test(s.textContent) ? s : null; }, 3000, 'the file attached to the box');
    check('attached to the box, after what was there', !!strip && /```\nnpm test\n```/.test(box.value), JSON.stringify(box.value));
  }

  async function popped() {
    var canvas = await until(function () { return q('.ca-termwin .ca-term-canvas'); }, 20000, 'the terminal in its window');
    await until(function () { return canvas.dataset.renderer; }, 5000, 'a renderer');
    var labels = await menuOf(canvas);
    check('its menu sends to its session and adds to its box there too', labels.indexOf(SEND) >= 0 && labels.indexOf(ADD) >= 0 && labels.indexOf(ADD_ALL) >= 0, labels.join(' | '));

    var grip = function (edge) { return q('.ca-termwin-grip.is-' + edge); };
    check('it has a right edge, a bottom edge and a corner to size it by', !!grip('side') && !!grip('far') && !!grip('corner'));
    var screen = q('.ca-termwin .ca-term-screen').getBoundingClientRect();
    var side = grip('side').getBoundingClientRect();
    var far = grip('far').getBoundingClientRect();
    check('clear of the terminal, which a drag there selects in', side.left >= screen.right && far.top >= screen.bottom, JSON.stringify({ side: side.left, right: screen.right, far: far.top, bottom: screen.bottom }));
    var w = window.innerWidth, h = window.innerHeight;
    var size = async function (edge, dx, dy) {
      var before = calls('kept_resize').length;
      var r = grip(edge).getBoundingClientRect();
      var x = r.left + r.width / 2, y = r.top + r.height / 2;
      pointer(grip(edge), 'pointerdown', x, y, 1);
      pointer(grip(edge), 'pointermove', x + dx, y + dy, 1);
      pointer(grip(edge), 'pointerup', x + dx, y + dy, 0);
      await until(function () { return calls('kept_resize').length > before; }, 3000, 'the window sized by its ' + edge);
      return last('kept_resize');
    };
    var r = await size('side', 60, 15);
    check('its right edge makes it wider', r.width === w + 60 && r.height === h, JSON.stringify(r));
    r = await size('far', 15, 40);
    check('its bottom edge makes it taller', r.width === w && r.height === h + 40, JSON.stringify(r));
    r = await size('corner', -30, 25);
    check('its corner does both', r.width === w - 30 && r.height === h + 25, JSON.stringify(r));
    check('a press on an edge gives it the keyboard', calls('kept_focus').length >= 3);
  }

  (async function () {
    try {
      var which = await until(function () { return q('.ca-termwin') ? 'popped' : q('.ca-sessinsp[data-shows-session]') ? 'drawer' : null; }, 30000, 'a terminal’s window or a session');
      await (which === 'popped' ? popped() : drawer());
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
