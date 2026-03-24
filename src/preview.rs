use gtk4::gio;
use gtk4::prelude::*;
use pulldown_cmark::{html, Options, Parser};
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use webkit6::prelude::*;
use webkit6::WebView;

struct RenderResult {
    html_body: String,
    base_uri: Option<String>,
}

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
  .preview-notice {{
    margin: 0 0 1.5rem 0;
    padding: 0.9rem 1rem;
    border-left: 4px solid #f9e2af;
    border-radius: 0 8px 8px 0;
    background: #2b2730;
    color: #f9e2af;
  }}
  .preview-error {{
    margin: 0 0 1.5rem 0;
    padding: 0.9rem 1rem;
    border-left: 4px solid #f38ba8;
    border-radius: 0 8px 8px 0;
    background: #2b1f29;
    color: #f5c2e7;
    white-space: pre-wrap;
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

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn base_uri_for_path(current_file: Option<&Path>) -> Option<String> {
    let base_dir = current_file
        .and_then(Path::parent)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());

    base_dir.map(|path| {
        let mut uri = gio::File::for_path(path).uri().to_string();
        if !uri.ends_with('/') {
            uri.push('/');
        }
        uri
    })
}

/// Convert markdown text to HTML using pulldown-cmark as a fallback when Pandoc is unavailable.
fn fallback_markdown_to_html(markdown: &str) -> String {
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

fn fallback_notice(message: &str, markdown: &str) -> String {
    format!(
        "<div class=\"preview-notice\">{}</div>{}",
        escape_html(message),
        fallback_markdown_to_html(markdown)
    )
}

fn render_error(message: &str) -> String {
    format!(
        "<div class=\"preview-error\">{}</div>",
        escape_html(message)
    )
}

fn pandoc_args() -> [&'static str; 5] {
    ["--from=markdown", "--to=html5", "--citeproc", "--mathml", "--wrap=none"]
}

fn render_markdown(
    markdown: String,
    current_file: Option<PathBuf>,
    webview: &WebView,
    generation: u64,
    active_process: &Rc<RefCell<Option<gio::Subprocess>>>,
    latest_generation: &Rc<Cell<u64>>,
) {
    let flags = gio::SubprocessFlags::STDIN_PIPE
        | gio::SubprocessFlags::STDOUT_PIPE
        | gio::SubprocessFlags::STDERR_PIPE;
    let launcher = gio::SubprocessLauncher::new(flags);

    if let Some(workdir) = current_file.as_deref().and_then(Path::parent) {
        launcher.set_cwd(workdir);
    }

    let args = pandoc_args();
    let argv: Vec<&OsStr> = std::iter::once(OsStr::new("pandoc"))
        .chain(args.iter().map(OsStr::new))
        .collect();

    let base_uri = base_uri_for_path(current_file.as_deref());

    let subprocess = match launcher.spawn(&argv) {
        Ok(process) => process,
        Err(err) => {
            let body = fallback_notice(
                &format!(
                    "Pandoc is not available. Install `pandoc` to enable citations and full Pandoc markdown in the preview. Falling back to CommonMark rendering.\n\n{}",
                    err
                ),
                &markdown,
            );
            let full_html = html_template(&body);
            webview.load_html(&full_html, base_uri.as_deref());
            return;
        }
    };

    *active_process.borrow_mut() = Some(subprocess.clone());

    let webview = webview.clone();
    let active_process = active_process.clone();
    let latest_generation = latest_generation.clone();
    let subprocess_for_callback = subprocess.clone();

    subprocess.communicate_utf8_async(Some(markdown.clone()), gio::Cancellable::NONE, move |result| {
        let is_current = active_process
            .borrow()
            .as_ref()
            .map(|active| active == &subprocess_for_callback)
            .unwrap_or(false);

        if !is_current {
            return;
        }

        *active_process.borrow_mut() = None;

        if generation != latest_generation.get() {
            return;
        }

        let render = match result {
            Ok((stdout, _stderr)) if subprocess_for_callback.is_successful() => RenderResult {
                html_body: stdout.unwrap_or_default().to_string(),
                base_uri,
            },
            Ok((_stdout, stderr)) => {
                let message = stderr
                    .as_ref()
                    .map(|s| s.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Pandoc exited with an error.");
                RenderResult {
                    html_body: render_error(message),
                    base_uri,
                }
            }
            Err(err) => RenderResult {
                html_body: render_error(&format!("Failed to run Pandoc: {err}")),
                base_uri,
            },
        };

        let full_html = html_template(&render.html_body);
        webview.load_html(&full_html, render.base_uri.as_deref());
    });
}

/// Set up live preview: when the editor buffer changes, update the WebView.
pub fn setup_live_preview(
    buffer: &sourceview5::Buffer,
    webview: &WebView,
    current_file: Rc<RefCell<Option<PathBuf>>>,
) {
    let pending_update: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let active_process: Rc<RefCell<Option<gio::Subprocess>>> = Rc::new(RefCell::new(None));
    let latest_generation = Rc::new(Cell::new(0_u64));

    let schedule_render = {
        let webview = webview.clone();
        let current_file = current_file.clone();
        let pending_update = pending_update.clone();
        let active_process = active_process.clone();
        let latest_generation = latest_generation.clone();

        move |text: String| {
            if let Some(source_id) = pending_update.borrow_mut().take() {
                source_id.remove();
            }

            if let Some(process) = active_process.borrow_mut().take() {
                process.force_exit();
            }

            let generation = latest_generation.get().wrapping_add(1);
            latest_generation.set(generation);

            let webview = webview.clone();
            let active_process = active_process.clone();
            let latest_generation = latest_generation.clone();
            let current_file = current_file.clone();
            let pending_update = pending_update.clone();

            let pending_for_timeout = pending_update.clone();
            let source_id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                    let current_path = current_file.borrow().clone();
                    render_markdown(
                        text,
                        current_path,
                        &webview,
                        generation,
                        &active_process,
                        &latest_generation,
                    );
                    *pending_for_timeout.borrow_mut() = None;
                });
            *pending_update.borrow_mut() = Some(source_id);
        }
    };

    let initial_text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    schedule_render(initial_text);

    buffer.connect_changed(move |buf| {
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        schedule_render(text);
    });
}
