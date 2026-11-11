// Checks that a file let go anywhere on the quick panel is attached (Quick.tsx `useFileDrop`
// over the whole page; quick.rs `lost_keys` and `dragged` keep the panel up while it comes from
// another app and bring the keyboard back once it lands, which the preview can't show): on the
// list, not only the box; in the shadow round the card; and on the chip the panel folds to.
//
//   scripts/preview.sh quick '' scripts/preview/quick-drop-check.js
(function () {
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return q('.ca-quick-fold'); };
  var names = function () { return all('.ca-quick-attached [title], .ca-quick-attached span').map(function (e) { return e.getAttribute('title') || e.textContent; }).join(' | '); };
  var imports = function () { return sent('import_attachment').map(function (a) { return String(a.path).split('/').pop(); }); };
  // What Tauri sends the page as a file is let go on its window: where, in the screen's pixels.
  var drop = function (path, x, y) {
    var scale = window.devicePixelRatio || 1;
    calls('plugin:event|listen')
      .filter(function (c) { return c[1].event === 'tauri://drag-drop'; })
      .forEach(function (c) { window['_' + c[1].handler]({ event: 'tauri://drag-drop', id: 1, payload: { paths: [path], position: { x: x * scale, y: y * scale } } }); });
  };
  var middle = function (el) { var r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };

  say('quick drop check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && !!input() && calls('plugin:event|listen').some(function (c) { return c[1].event === 'tauri://drag-drop'; }); }, 30000, 'the quick panel, listening for drops')
    .then(function () { return wait(200); })
    .then(function () {
      var list = q('.ca-quick-list');
      check('the panel lists something to drop on', !!list && list.getBoundingClientRect().height > 40, list && String(list.getBoundingClientRect().height));
      var at = middle(list);
      drop('/Users/me/Pictures/kestrel-nest.png', at[0], at[1]);
      return wait(300);
    })
    .then(function () {
      check('a file let go on the list is kept', imports().indexOf('kestrel-nest.png') >= 0, imports().join(', '));
      check('and goes into the strip under the box', names().indexOf('kestrel-nest.png') >= 0, names());
      // The shadow round the card is the page's too: beside it, since the preview's page is
      // shorter than the card and its shadow below.
      var card = q('.ca-quick').getBoundingClientRect();
      drop('/Users/me/Pictures/atlas-map.png', card.left - 8, card.top + 100);
      return wait(300);
    })
    .then(function () {
      check('one let go in the shadow round the card is kept too', names().indexOf('atlas-map.png') >= 0, names());
      type(input(), 'Put the kestrel nest on the atlas map\nand mark the trail');
      return wait(100);
    })
    .then(function () {
      var size = window.__quickFold();
      check('a prompt folds the panel to its chip', !!size && !!chip(), JSON.stringify(size));
      var at = middle(chip());
      drop('/Users/me/Pictures/northwind-shore.png', at[0], at[1]);
      return wait(300);
    })
    .then(function () {
      check('a file let go on the chip is kept', imports().indexOf('northwind-shore.png') >= 0, imports().join(', '));
      // Codealong opens the panel again as it was (quick.rs `dragged`).
      window.__quickOpen({ keep: true }, null);
      return wait(200);
    })
    .then(function () {
      check('opened again, the panel has all three', ['kestrel-nest.png', 'atlas-map.png', 'northwind-shore.png'].every(function (n) { return names().indexOf(n) >= 0; }), names());
      check('and the prompt as written', input().value.indexOf('Put the kestrel nest') === 0, input().value);
      done();
    })
    .catch(stopped);
})();
