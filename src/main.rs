mod app;
mod editor;
mod file_ops;
mod preview;
mod sidebar;
mod sync_scroll;
mod vim;

use libadwaita as adw;
use adw::prelude::*;
use adw::Application;

const APP_ID: &str = "dev.agentic.md";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(app::build_ui);

    app.run();
}
