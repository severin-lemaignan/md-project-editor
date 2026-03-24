use gtk4::prelude::*;
use pulldown_cmark::{Options, Parser, html};
use std::cell::RefCell;
use std::rc::Rc;
use webkit6::prelude::*;
use webkit6::WebView;

/// HTML template that wraps the rendered markdown content.
fn html_template(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  :root {{
    color-scheme: dark;
  }}
  * {{
    box-sizing: border-box;
  }}
  body {{
    font-family: -apple-system, 'Segoe UI', 'Roboto', 'Helvetica Neue', sans-serif;
    font-size: 15px;
    line-height: 1.7;
    color: #e0e0e0;
    background-color: #1e1e2e;
    padding: 24px 32px;
    margin: 0;
    max-width: 100%;
    overflow-x: hidden;
  }}
  h1, h2, h3, h4, h5, h6 {{
    color: #cdd6f4;
    margin-top: 1.4em;
    margin-bottom: 0.6em;
    font-weight: 600;
  }}
  h1 {{
    font-size: 2em;
    border-bottom: 2px solid #45475a;
    padding-bottom: 0.3em;
  }}
  h2 {{
    font-size: 1.5em;
    border-bottom: 1px solid #45475a;
    padding-bottom: 0.25em;
  }}
  a {{
    color: #89b4fa;
    text-decoration: none;
  }}
  a:hover {{
    text-decoration: underline;
  }}
  code {{
    font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
    background: #313244;
    color: #f38ba8;
    padding: 0.15em 0.4em;
    border-radius: 4px;
    font-size: 0.9em;
  }}
  pre {{
    background: #181825;
    border: 1px solid #45475a;
    border-radius: 8px;
    padding: 16px;
    overflow-x: auto;
  }}
  pre code {{
    background: transparent;
    color: #cdd6f4;
    padding: 0;
  }}
  blockquote {{
    margin: 1em 0;
    padding: 0.5em 1em;
    border-left: 4px solid #89b4fa;
    background: #181825;
    border-radius: 0 8px 8px 0;
    color: #bac2de;
  }}
  table {{
    border-collapse: collapse;
    width: 100%;
    margin: 1em 0;
  }}
  th, td {{
    border: 1px solid #45475a;
    padding: 8px 12px;
    text-align: left;
  }}
  th {{
    background: #313244;
    color: #cdd6f4;
    font-weight: 600;
  }}
  tr:nth-child(even) {{
    background: #1e1e2e;
  }}
  tr:nth-child(odd) {{
    background: #181825;
  }}
  hr {{
    border: none;
    border-top: 1px solid #45475a;
    margin: 2em 0;
  }}
  ul, ol {{
    padding-left: 1.5em;
  }}
  li {{
    margin: 0.3em 0;
  }}
  img {{
    max-width: 100%;
    border-radius: 8px;
  }}
  ::selection {{
    background: #45475a;
    color: #cdd6f4;
  }}
  ::-webkit-scrollbar {{
    width: 8px;
  }}
  ::-webkit-scrollbar-track {{
    background: #1e1e2e;
  }}
  ::-webkit-scrollbar-thumb {{
    background: #45475a;
    border-radius: 4px;
  }}
  ::-webkit-scrollbar-thumb:hover {{
    background: #585b70;
  }}
</style>
<script>
  function scrollToPercent(pct) {{
    var maxScroll = document.documentElement.scrollHeight - window.innerHeight;
    if (maxScroll > 0) {{
      window.scrollTo(0, maxScroll * pct);
    }}
  }}
</script>
</head>
<body>
{body}
</body>
</html>"#
    )
}

/// Convert markdown text to HTML using pulldown-cmark.
fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Set up live preview: when the editor buffer changes, update the WebView.
pub fn setup_live_preview(buffer: &sourceview5::Buffer, webview: &WebView) {
    // Render initial content
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    let html = html_template(&markdown_to_html(&text));
    webview.load_html(&html, None);

    // Debounce timer handle
    let pending_update: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let webview_clone = webview.clone();
    let pending_clone = pending_update.clone();

    buffer.connect_changed(move |buf| {
        // Cancel any pending update
        if let Some(source_id) = pending_clone.borrow_mut().take() {
            source_id.remove();
        }

        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        let wv = webview_clone.clone();
        let pending = pending_clone.clone();

        // Schedule update after a short delay (150ms debounce)
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            move || {
                let html_body = markdown_to_html(&text);
                let full_html = html_template(&html_body);
                wv.load_html(&full_html, None);

                // Clear the pending handle
                *pending.borrow_mut() = None;
            },
        );
        *pending_clone.borrow_mut() = Some(source_id);
    });
}
