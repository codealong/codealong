// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts) and, when present,
// pr-detail.json (pr_detail, for any PR).
//
// Native menus aren't drawn, but they're made and "shown" as if they were, so a right-click
// runs to the end. `window.__menu()` lists the last menu's items as [label, pick] pairs;
// pick() runs an item as choosing it would.
window.__calls = [];
window.__TAURI_INTERNALS__ = {
  invoke: function (cmd, args) {
    window.__calls.push([cmd, args]);
    var file = { get_board: './board.json', get_accounts: './accounts.json', pr_detail: './pr-detail.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    return Promise.resolve(null);
  },
  transformCallback: function (cb) {
    var id = Math.floor(Math.random() * 1e9);
    window['_' + id] = cb;
    return id;
  },
  unregisterCallback: function () {},
  convertFileSrc: function (p) { return p; },
  metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
};
window.__menu = function () {
  var made = window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; });
  var last = made[made.length - 1];
  var items = last ? last[1].options.items || [] : [];
  return items
    .filter(function (i) { return i.text; })
    .map(function (i) {
      return [i.text, function () { window['_' + i.handler.id]({ index: 0, message: i.id }); }];
    });
};
