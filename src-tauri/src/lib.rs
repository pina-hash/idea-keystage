//! IDEA Keystage desktop app: wires the platform input layer, the core processor,
//! the overlay hosts, the local server and the settings window together.

pub mod app;
pub mod platform;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running IDEA Keystage");
}
