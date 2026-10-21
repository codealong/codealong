// Opens the dialog that makes a schedule (ui/ScheduleDialog.tsx), for a screenshot of it:
//   PREVIEW_INIT=scripts/preview/schedule-dialog-init.js PREVIEW_NAME=schedule-dialog PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules
(function () {
  var open = setInterval(function () {
    var button = document.querySelector('.ca-schednew');
    if (!button) return;
    clearInterval(open);
    button.click();
  }, 100);
})();
