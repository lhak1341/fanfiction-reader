// Injected into the `fetch-page` webview before the page's own scripts run
// (see fetch_page's doc comment in fetch.rs for why a script is injected at
// all). Reports the rendered page back to Rust exactly once per navigation.
(function () {
    var reported = false;
    function report() {
        if (reported || !window.__TAURI__) return;
        reported = true;
        console.log('[fetch] reporting page (readyState=' + document.readyState + ')');
        window.__TAURI__.core.invoke('report_fetch_result', {
            html: document.documentElement.outerHTML,
            pageUrl: window.location.href,
            title: document.title
        }).then(function () {
            console.log('[fetch] report_fetch_result invoke resolved');
        }).catch(function (e) {
            console.error('[fetch] report_fetch_result invoke rejected:', e);
        });
    }
    // DOMContentLoaded fires once HTML is parsed, without waiting on
    // slow/hanging subresources (ads, trackers, long-polling) the way
    // `load` or Tauri's own Finished event do. A timeout fallback covers
    // the rest.
    document.addEventListener('DOMContentLoaded', report);
    setTimeout(report, 4000);
})();
