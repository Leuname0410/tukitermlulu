#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sessions;
mod terminal;

use tauri::Manager;
use tuki_adapters::ssh_transport::RusshTransport;

use crate::terminal::{AppState, close_tab, connect, disconnect, terminal_resize, terminal_write};

fn main() {
    if let Err(error) = tauri::Builder::default()
        .setup(|app| {
            let known_hosts_path = app.path().app_data_dir()?.join("known_hosts");
            app.manage(AppState::new(RusshTransport::new(), known_hosts_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            terminal_write,
            terminal_resize,
            disconnect,
            close_tab
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
        std::process::exit(1);
    }
}
