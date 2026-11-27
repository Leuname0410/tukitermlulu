#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sessions;
mod terminal;
mod transfers;

use std::fs;

use tauri::Manager;
use tuki_adapters::session_repository::SqliteSessionRepository;
use tuki_adapters::ssh_transport::RusshTransport;

use crate::sessions::{
    create_session, delete_folder, delete_session, duplicate_session, list_folders, list_sessions,
    save_folder, update_session,
};
use crate::terminal::{AppState, close_tab, connect, disconnect, terminal_resize, terminal_write};

fn main() {
    if let Err(error) = tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let known_hosts_path = data_dir.join("known_hosts");
            let repository = SqliteSessionRepository::open(&data_dir.join("sessions.db"))?;
            app.manage(AppState::new(
                RusshTransport::new(),
                known_hosts_path,
                repository,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            terminal_write,
            terminal_resize,
            disconnect,
            close_tab,
            list_sessions,
            create_session,
            update_session,
            duplicate_session,
            delete_session,
            list_folders,
            save_folder,
            delete_folder
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
        std::process::exit(1);
    }
}
