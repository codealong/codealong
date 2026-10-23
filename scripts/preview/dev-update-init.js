// Shows Codealong Dev's Update in the sidebar, origin/main having moved past the build (pushed as
// updates.rs `dev_check` would), for a screenshot (preview.sh's sessions view).
//   PREVIEW_INIT=scripts/preview/dev-update-init.js PREVIEW_NAME=dev-update scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/dev-update-init.js PREVIEW_NAME=dev-update scripts/preview.sh sessions
(function () {
  window.__update = { current: '0.6.0', state: 'off', behind: { built: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b', commits: 3, source: '/Users/me/projects/codealong', checkedAt: Date.now() } };
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    if (document.querySelector('.ca-side') && window.__emit) {
      window.__emit('update-status', window.__update);
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
