// Checks that a window's own background failures are notices (notices.ts `reportNotice`,
// `clearNotice`). In a board view: where each account can start a session, read at launch for
// Review with (App.tsx), fails, and its notice shows in the accounts bar with what failed and
// why; once it works, a PR row's menu reads it again and the notice goes; and a notice another
// window hands on shows there too, until that window says it's over. In the quick panel, which
// has no accounts bar, the same failure and one reading your other devices are handed on.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/background-notices-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/background-notices-check.js
(function () {
  // This runs before the page's own scripts, so the app asks through this from the start.
  var failing = { start_options: true, devices: true };
  var tauri = window.__TAURI_INTERNALS__;
  var invoke = tauri.invoke;
  tauri.invoke = function (cmd) {
    if (failing[cmd]) return Promise.reject('the engine is busy');
    return invoke.apply(this, arguments);
  };
  var START = 'Can’t read where sessions start';

  if (window.__CODEALONG_QUICK__) {
    // What the panel handed on.
    var handed = function () {
      return sent('plugin:event|emit')
        .filter(function (a) { return a.event === 'window-notice' && a.payload.notice; })
        .map(function (a) { return a.payload.notice; });
    };
    var keyed = function (key) { return handed().filter(function (n) { return n.key === key; })[0]; };
    say('background notices check: waiting');
    until(function () { return keyed('start-options'); }, 20000, 'the start options notice handed on')
      .then(function () { return wait(1000); })
      .then(function () {
        var n = keyed('start-options');
        check('a failed read of where sessions start is handed on', n.title === START && /: the engine is busy$/.test(n.text), JSON.stringify(n));
        var devices = keyed('quick-devices');
        if (devices) check('and one of your other devices', /: the engine is busy$/.test(devices.text), JSON.stringify(devices));
        else note('your other devices are off here, so the panel didn’t read them');
        check('each once', handed().length === (devices ? 2 : 1), JSON.stringify(handed()));
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The notices' titles and texts, from their card: it opens, is read, and closes again, each
  // step once the page shows the one before.
  var read = function () {
    var chip = q('.ca-notice-open');
    if (!chip) return Promise.resolve({ titles: [], texts: [] });
    if (!q('.ca-notice-card')) chip.click();
    return until(function () { return q('.ca-notice-card'); }, 3000, 'the notices’ card').then(function (card) {
      var got = {
        titles: all('.ca-notice-title', card).map(function (el) { return el.textContent; }),
        texts: all('.ca-notice-text', card).map(function (el) { return el.textContent; }),
      };
      q('.ca-notice-open').click();
      return until(function () { return !q('.ca-notice-card'); }, 3000, 'the card to close').then(function () { return got; });
    });
  };
  // Resolves once `test` holds of what the notices say, asking again until it does.
  var once = function (test, what) {
    var start = Date.now();
    return new Promise(function (res, rej) {
      (function poll() {
        read().then(function (got) {
          if (test(got)) return res(got);
          if (Date.now() - start > 15000) return rej(new Error('timeout: ' + what + ' (' + got.titles.join(' | ') + ')'));
          setTimeout(poll, 200);
        }, rej);
      })();
    });
  };
  var shows = function (got, title) { return got.titles.indexOf(title) >= 0; };
  var other = { key: 'conversation:x', title: 'A conversation isn’t updating', text: 'Couldn’t read what’s new in “x”: gone', until: null };
  say('background notices check: waiting');
  once(function (got) { return shows(got, START); }, 'the start options notice')
    .then(function (got) {
      check('a failed read of where sessions start is a notice', true);
      check('it says what failed and why', /the last answer read stands: the engine is busy/.test(got.texts.join(' | ')), got.texts.join(' | '));
      // Another window hands one on, and then says it's over.
      window.__emit('window-notice', { notice: other });
      return once(function (now) { return shows(now, other.title); }, 'the notice handed on');
    })
    .then(function () {
      check('one another window hands on shows here', true);
      window.__emit('window-notice', { clear: other.key });
      return once(function (now) { return !shows(now, other.title); }, 'the handed-on notice to go');
    })
    .then(function (got) {
      check('and goes when that window says so', true);
      check('the start options notice stays until its read works', shows(got, START), got.titles.join(' | '));
      failing.start_options = false;
      var prs = all('.ca-nav').filter(function (b) { return /PRs/.test(b.textContent); })[0];
      prs.click();
      return until(function () { return q('.ca-prrow'); }, 10000, 'a PR row');
    })
    .then(function (row) {
      // Its menu has Review with, which reads where sessions start.
      return menuOf(row).then(function () {
        return until(function () { return !q('.ca-notice-open'); }, 10000, 'the start options notice to go');
      });
    })
    .then(function () { check('and it goes once a PR’s menu reads them', true); })
    .then(done)
    .catch(stopped);
})();
