// Checks the training flags in the accounts bar (ui/UsageBar.tsx, training.ts): an account whose
// provider may train on it, or where nothing's chosen, has a flag joined to its chip, the others
// none; its card says why and opens the provider's page; Turn off training asks for it, closes
// the card and says so; one that didn't take says why in the card. Then Settings › Accounts tags
// each account it can tell about.
//   PREVIEW_TRAINING=scripts/preview/training-sample.json PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/training-check.js
(function () {
  var flagOf = function (account) {
    return all('.ca-usagebar-accts .ca-uchip-pair').filter(function (p) { return (q('.ca-uchip-name', p) || {}).textContent === account; }).map(function (p) { return q('.ca-uchip-flag', p); })[0];
  };
  var button = function (text) { return all('.ca-training-card button').filter(function (b) { return b.textContent === text; })[0]; };
  var toast = function () { return (q('.ca-toast') || {}).textContent || ''; };

  until(function () { return q('.ca-usagebar-accts .ca-uchip-flag'); }, 20000)
    .then(function () {
      var flagged = all('.ca-usagebar-accts .ca-uchip-pair .ca-uchip-name').map(function (n) { return n.textContent; });
      check('only accounts that may be trained on are flagged', flagged.join(',') === 'claude,claude2', flagged.join(','));
      check('the flag says what', flagOf('claude').textContent === 'Training on' && flagOf('claude2').textContent === 'Training not set', flagOf('claude').textContent + ' / ' + flagOf('claude2').textContent);
      check('its chip is joined to it', flagOf('claude').previousElementSibling.classList.contains('is-joined'));
      flagOf('claude').click();
      return until(function () { return q('.ca-training-card'); }, 3000);
    })
    .then(function () {
      check('the card says why', q('.ca-training-title').textContent === 'Training is on for claude' && /^Help improve Claude is on/.test(q('.ca-training-text').textContent), q('.ca-training-title').textContent);
      check('and when it was read', /^Read /.test((q('.ca-training-asof') || {}).textContent || ''), (q('.ca-training-asof') || {}).textContent);
      button('Open settings').click();
      check('Open settings opens the provider’s page', sent('open_url').some(function (a) { return a && a.url === 'https://claude.ai/settings/data-privacy-controls'; }), JSON.stringify(sent('open_url')));
      button('Turn off training').click();
      return until(function () { return !q('.ca-training-card'); }, 3000);
    })
    .then(function () {
      check('Turn off training asks for it', sent('stop_training').some(function (a) { return a.name === 'claude'; }), JSON.stringify(sent('stop_training')));
      return until(function () { return toast().indexOf('Training is off for claude.') >= 0; }, 3000);
    })
    .then(function () {
      check('and says it went', true);
      flagOf('claude2').click();
      return until(function () { return q('.ca-training-card'); }, 3000);
    })
    .then(function () {
      button('Turn off training').click();
      return until(function () { return q('.ca-training-error'); }, 3000);
    })
    .then(function () {
      check('one that didn’t take says why', /still reads as on/.test(q('.ca-training-error').textContent) && !!q('.ca-training-card'), q('.ca-training-error').textContent);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
      return until(function () { return !q('.ca-training-card'); }, 3000);
    })
    .then(function () {
      check('a click elsewhere puts the card away', true);
      all('.ca-usagebar-end button').filter(function (b) { return /Accounts/.test(b.textContent); })[0].click();
      return until(function () { return q('#ca-acct-claude .ca-acct-tags'); }, 5000);
    })
    .then(function () {
      var tags = function (account) { return all('#ca-acct-' + account + ' .ca-acct-tags .ca-tag').map(function (t) { return t.textContent; }).slice(1).join(','); };
      check('Settings tags each account it can tell about', tags('claude') === 'Training on' && tags('codex') === 'Not trained on' && tags('opencode') === '', [tags('claude'), tags('codex'), tags('opencode')].join(' / '));
      var actions = all('#ca-acct-claude .ca-acct-actions button').map(function (b) { return b.textContent; });
      check('and offers to turn it off there', actions.indexOf('Turn off training') >= 0 && actions.indexOf('Open settings') >= 0, actions.join(' | '));
    })
    .then(done)
    .catch(function (e) { stopped(e.message); });
})();
