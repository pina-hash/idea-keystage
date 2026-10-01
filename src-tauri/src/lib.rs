//! IDEA Keystage desktop app: wires the platform input layer, the core processor,
//! the overlay hosts, the local server and the settings window together.

pub mod app;
pub mod platform;

/// Starts the app. Plugin order matters: single-instance first, so a second
/// launch hands over to the running app (which brings its settings window
/// forward) before anything else starts.
pub fn run() {
    let autostarted = std::env::args().any(|a| a == app::AUTOSTART_ARG);
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app::on_second_instance(app)
        }))
        .plugin(app::hotkeys_plugin())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(app::AUTOSTART_ARG)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(app::commands::handler())
        .setup(move |a| app::setup(a, autostarted).map_err(Into::into))
        .build(tauri::generate_context!());
    match built {
        Ok(a) => a.run(app::on_run_event),
        Err(e) => {
            tracing::error!(error = %e, "IDEA Keystage could not start");
            std::process::exit(1);
        }
    }
}
