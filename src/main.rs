mod app;
mod citation_completion;
mod editor;
mod file_ops;
mod preview;
mod sidebar;
mod sync_scroll;
mod vim;

use adw::prelude::*;
use adw::Application;
use libadwaita as adw;

const APP_ID: &str = "org.skadge.academicassistant";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(app::build_ui);

    app.run();
}
