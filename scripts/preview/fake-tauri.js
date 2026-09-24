// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts) and, when present,
// pr-detail.json (pr_detail, for any PR).
window.__calls = [];
window.__TAURI_INTERNALS__ = {
  invoke: function (cmd, args) {
    window.__calls.push([cmd, args]);
    var file = { get_board: './board.json', get_accounts: './accounts.json', pr_detail: './pr-detail.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
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
