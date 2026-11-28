// Checks the unified Board Map layout (Kanban | Map | Canvas in BoardBar): switching to Map renders
// clustered session cards with action beacons and zero overlaps, the action hopper selects an
// actionable card, Action only dims quiet cards, clicking a cluster opens the overview drawer,
// shelving removes a card through `board_op: remove`, switching between multiple boards updates the
// map, and switching back to Canvas restores CanvasView cleanly.
//   PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh canvas '' scripts/preview/working-set-check.js
//   PREVIEW_BOARDS=scripts/preview/boards-sample.json scripts/preview.sh canvas '' scripts/preview/working-set-check.js
(function () {
  var layoutBtn = function (label) {
    return all('.ca-boardbar-layout button').filter(function (b) { return b.textContent.trim() === label; })[0];
  };
  var subboards = function () {
    return all('.ca-side .ca-side-subboard:not(.ca-side-newboard)');
  };
  var key = function (k, code) {
    var e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: k, code: code || k });
    (document.activeElement || document.body).dispatchEvent(e);
    return wait(150);
  };

  until(function () { return q('.ca-boardbar-layout'); }, 10000, 'board bar layout switcher')
    .then(function () {
      var mapBtn = layoutBtn('Map');
      check('BoardBar offers Kanban | Map | Canvas switcher', !!layoutBtn('Kanban') && !!mapBtn && !!layoutBtn('Canvas'));
      mapBtn.click();
      return until(function () { return q('.ca-ws'); }, 5000, 'working set map');
    })
    .then(function () {
      var cards = all('.ca-ws-card');
      var hubs = all('.ca-ws-hub');
      var clusters = all('.ca-ws-cluster');
      check('renders working-set session cards for the board', cards.length > 0, cards.length + ' cards, ' + hubs.length + ' hubs, ' + clusters.length + ' clusters');
      note('board: ' + (q('.ca-boardbar-name span') ? q('.ca-boardbar-name span').textContent : '?') + ' · ' + cards.length + ' sessions, ' + hubs.length + ' hubs, ' + clusters.length + ' clusters');
      note('clusters: ' + all('.ca-ws-cluster-title').map(function (t) { return t.textContent; }).join(' | '));

      // Verify zero overlapping cards
      var rects = cards.concat(hubs).map(function (el) { return el.getBoundingClientRect(); });
      var overlapCount = 0;
      for (var i = 0; i < rects.length; i++) {
        for (var j = i + 1; j < rects.length; j++) {
          var a = rects[i], b = rects[j];
          if (a.left + 2 < b.right && b.left + 2 < a.right && a.top + 2 < b.bottom && b.top + 2 < a.bottom) overlapCount++;
        }
      }
      check('zero overlapping cards on map', overlapCount === 0, overlapCount + ' overlaps');

      // Action hopper selects an actionable session
      var hop = q('.ca-ws-hop');
      if (hop) {
        hop.click();
        return until(function () { return q('.ca-ws-card.is-selected'); }, 3000, 'selected actionable card').then(function () {
          check('action hopper selects an actionable card', !!q('.ca-ws-card.is-selected'));
        });
      }
      return wait(100);
    })
    .then(function () {
      // Spotlight toggles cleanly
      var spot = all('.ca-ws-pill').filter(function (b) { return /Action only/.test(b.textContent); })[0];
      check('Action only spotlight button exists', !!spot);
      spot.click();
      return wait(150).then(function () {
        check('Action only button reflects pressed state', spot.getAttribute('aria-pressed') === 'true');
        spot.click();
        return wait(150);
      });
    })
    .then(function () {
      // Clicking a cluster header opens the cluster overview drawer
      var head = q('.ca-ws-cluster-head');
      if (!head) return wait(50);
      var r = head.getBoundingClientRect();
      head.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: r.left + 10, clientY: r.top + 8 }));
      head.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1, clientX: r.left + 10, clientY: r.top + 8 }));
      return until(function () { return q('.ca-ws-panel'); }, 3000, 'cluster overview panel').then(function () {
        check('clicking cluster header opens Who’s doing what overview', all('.ca-ws-panel-row').length > 0, all('.ca-ws-panel-row').length + ' rows');
        return key('Escape', 'Escape');
      });
    })
    .then(function () {
      // Shelving removes the card from the board via board_op: remove
      var before = all('.ca-ws-card').length;
      var removesBefore = sent('board_op').filter(function (a) { return a.op.op === 'remove'; }).length;
      var shelve = q('.ca-ws-card .ca-ws-shelve-btn');
      if (shelve) {
        shelve.click();
        return until(function () { return all('.ca-ws-card').length === before - 1; }, 3000, 'shelved card removed').then(function () {
          var removesAfter = sent('board_op').filter(function (a) { return a.op.op === 'remove'; }).length;
          check('shelving removes card from board via board_op', removesAfter === removesBefore + 1, 'removes: ' + removesBefore + ' -> ' + removesAfter);
        });
      }
      return wait(50);
    })
    .then(function () {
      // When multiple boards exist, switching to another board updates the Map cleanly
      var subs = subboards();
      if (subs.length < 2) return wait(50);
      var firstName = q('.ca-boardbar-name span').textContent;
      subs[1].click();
      return until(function () { return q('.ca-boardbar-name span') && q('.ca-boardbar-name span').textContent !== firstName; }, 3000, 'second board shown').then(function () {
        var secondName = q('.ca-boardbar-name span').textContent;
        check('switching to another board keeps Map view on the new board', !!q('.ca-ws') && secondName !== firstName, firstName + ' -> ' + secondName);
      });
    })
    .then(function () {
      // Switching back to Canvas in BoardBar restores CanvasView cleanly
      layoutBtn('Canvas').click();
      return until(function () { return q('.ca-cv'); }, 5000, 'board canvas view').then(function () {
        check('switching back to Canvas restores CanvasView', !!q('.ca-cv') && !q('.ca-ws'));
      });
    })
    .then(done)
    .catch(stopped);
})();
