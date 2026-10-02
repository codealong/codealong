// What every check script starts from. preview.sh loads this ahead of a check (and of a
// PREVIEW_INIT script), after fake-tauri.js, so a check only says what it proves:
//
//   until(function () { return q('.ca-thing'); }, 20000)
//     .then(function (thing) {
//       rightClick(thing);
//       check('its menu pins it', menuLabels().indexOf('Pin') >= 0, menuLabels().join(' | '));
//     })
//     .then(done)
//     .catch(stopped);
//
// done() puts the one result in the page's title, which preview.sh prints and exits by:
//
//   done: PASS 7/7
//   done: FAIL 5/7 <name>: <why>; <name>: <why>
//
// followed, when there are any, by ` ## unanswered: …`, the commands fake-tauri.js had no
// answer for (each resolved null; a new command that renders as nothing shows up here), and
// ` ## note: …`, what the check wanted said (note(): a step skipped for want of data, a menu to
// read). A check that proved nothing fails: a script that can't find what it needs throws, or
// calls stopped() itself, and says what was missing.
//
// It works on Chrome's virtual time and on the real clock (PREVIEW_WAIT) alike: waits go by
// timers and Date.now(), never by frames or performance.now(), which stand still there. The app
// rewrites the title as its counts change, so what a check says is put back whenever it does.
(function () {
  var title = null;
  var keep = function () {
    if (title === null || document.title === title) return;
    document.title = title;
    title = document.title;
  };
  setInterval(keep, 50);
  new MutationObserver(keep).observe(document.head, { subtree: true, childList: true, characterData: true });

  // Where the check has got to, for whoever reads the title before it's done. The page gives a
  // title back with its white space run together, so that's how it's kept: one that never read
  // back the same would be put back for ever.
  window.say = function (text) {
    title = String(text).replace(/\s+/g, ' ').trim();
    keep();
  };

  window.wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  // Resolves with what `f` returns once that's something, or fails after `ms`, saying what it
  // waited for: `what`, or else what `f` asks.
  window.until = function (f, ms, what) {
    var start = Date.now();
    return new Promise(function (res, rej) {
      (function poll() {
        var v = f();
        if (v) return res(v);
        if (Date.now() - start > ms) {
          return rej(new Error('timeout: ' + (what || String(f).replace(/\s+/g, ' ').replace(/^function \(\) \{ return /, '').replace(/;? \}$/, '').slice(0, 120))));
        }
        setTimeout(poll, 10);
      })();
    });
  };

  var results = [];
  var notes = [];
  // One thing the check proves. `why` is what it found, said only when it fails.
  window.check = function (name, ok, why) { results.push({ name: name, ok: !!ok, why: why }); };
  // Something to say beside the result that isn't a failure.
  window.note = function (text) { notes.push(String(text)); };
  // Chrome cuts a title at 4,096 characters, and ` ## ` is what sets the parts of a result apart.
  var short = function (text, most) {
    text = String(text).replace(/ ##+ /g, ' # ');
    return text.length > most ? text.slice(0, most - 1) + '…' : text;
  };
  window.done = function () {
    var failed = results.filter(function (r) { return !r.ok; });
    var passed = results.length - failed.length;
    if (results.length === 0) failed = [{ name: 'nothing was checked' }];
    var text = (failed.length ? 'FAIL ' : 'PASS ') + passed + '/' + results.length;
    if (failed.length) {
      text += ' ' + short(failed.map(function (r) { return r.name + (r.why == null || r.why === '' ? '' : ': ' + short(r.why, 300)); }).join('; '), 2400);
    }
    if ((window.__unknown || []).length) text += ' ## unanswered: ' + window.__unknown.join(', ');
    // preview.sh sizes the window so the page is the size asked for; a Chrome that frames its
    // windows differently would show up here.
    var asked = window.__PREVIEW_SIZE__;
    if (asked && asked !== innerWidth + ',' + innerHeight) notes.push('the page is ' + innerWidth + ' by ' + innerHeight + ', not the ' + asked.replace(',', ' by ') + ' asked for');
    if (notes.length) text += ' ## note: ' + short(notes.join('; '), 3000);
    window.say('done: ' + text.slice(0, 4000));
  };
  // The check couldn't go on: a failure of its own, then the result. For a promise chain's catch.
  window.stopped = function (e) {
    window.check('stopped', false, e && e.message ? e.message : String(e));
    window.done();
  };

  // The commands the page sent, as fake-tauri.js recorded them: [name, args, options] each.
  window.calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  // Their arguments alone, and the latest one's (null when it was never sent).
  window.sent = function (cmd) { return window.calls(cmd).map(function (c) { return c[1]; }); };
  window.last = function (cmd) {
    var c = window.calls(cmd);
    return c.length ? c[c.length - 1][1] : null;
  };

  window.q = function (sel, root) { return (root || document).querySelector(sel); };
  window.all = function (sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); };
  // Types into a field React controls: the value goes in under React, then the event it hears.
  window.type = function (el, text) {
    var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  window.key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };

  // Native menus aren't drawn, but the page makes and "shows" them: a right-click runs to the
  // end, and the menu it asked for is the last one made.
  window.rightClick = function (el, init) { el.dispatchEvent(new MouseEvent('contextmenu', Object.assign({ bubbles: true, cancelable: true }, init))); };
  // How many menus the page has made, to tell that a click made one.
  window.menus = function () { return window.calls('plugin:menu|new').length; };
  // The last menu as the page made it: items with `text`, `id`, `handler`, `items`…
  window.lastMenu = function () {
    var m = window.calls('plugin:menu|new');
    return m.length ? m[m.length - 1][1].options.items : [];
  };
  // Picks one of those items, as choosing it would.
  window.pick = function (item) { window['_' + item.handler.id]({ index: 0, message: item.id }); };
  // The last menu's labels as fake-tauri.js's __menu lists them ("Parent › Item", " ✓" when
  // checked), and one label's pick().
  window.menuLabels = function () { return window.__menu().map(function (x) { return x[0]; }); };
  window.menuItem = function (label) { return (window.__menu().filter(function (x) { return x[0] === label; })[0] || [])[1]; };
  // Right-clicks and waits for the menu that asks for: resolves with its labels.
  window.menuOf = function (el, init) {
    var before = window.menus();
    window.rightClick(el, init);
    return window.until(function () { return window.menus() > before; }, 3000, 'no menu').then(window.menuLabels);
  };
})();
