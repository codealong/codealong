// Opens Canvas directly in the dynamic Working Set Map mode.
//   PREVIEW_BOARDS=scripts/preview/boards-everything.json PREVIEW_INIT=scripts/preview/working-set-init.js scripts/preview.sh canvas ''
(function () {
  localStorage.setItem('codealong:canvas-mode', 'working-set');
})();
