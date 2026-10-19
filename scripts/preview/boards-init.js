// Shows a board of the sample's (boards-sample.json) in every project: This week, whose Kanban has
// columns of its own (Next, then the stages), or with `window.__boardShown` set earlier, that one.
//   PREVIEW_BOARDS=scripts/preview/boards-sample.json PREVIEW_INIT=scripts/preview/boards-init.js scripts/preview.sh kanban
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.board', JSON.stringify({ 'All projects': window.__boardShown || 'b-sample-week' }));
})();
