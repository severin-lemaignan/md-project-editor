use gtk4::prelude::*;
use gtk4::ScrolledWindow;
use webkit6::prelude::*;
use webkit6::WebView;

/// Set up synchronized scrolling from the editor's ScrolledWindow to the WebView.
///
/// When the editor scrolls, compute the scroll percentage and send it to the
/// WebView via JavaScript.
pub fn setup_sync_scroll(editor_scrolled: &ScrolledWindow, webview: &WebView) {
    let adj = editor_scrolled.vadjustment();
    let wv = webview.clone();

    adj.connect_value_changed(move |adj| {
        let upper = adj.upper();
        let lower = adj.lower();
        let page_size = adj.page_size();
        let value = adj.value();

        let range = upper - lower - page_size;
        if range <= 0.0 {
            return;
        }

        let pct = (value - lower) / range;
        let pct = pct.clamp(0.0, 1.0);

        // Call the JavaScript function in the preview HTML
        let js = format!("scrollToPercent({pct});");
        wv.evaluate_javascript(&js, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
    });
}
