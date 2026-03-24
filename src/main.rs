mod app;
mod editor;
mod preview;
mod sync_scroll;
mod vim;

use gtk4::prelude::*;
use gtk4::Application;

const APP_ID: &str = "dev.agentic.md";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(app::build_ui);

    app.run();
}
