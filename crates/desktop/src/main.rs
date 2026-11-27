#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diagnostics;
mod logging;
mod sessions;
mod sftp;
mod shell_hook;
mod terminal;
mod transfers;

use std::fs;
use std::sync::Arc;

use tauri::Manager;
use tuki_adapters::session_repository::SqliteSessionRepository;
use tuki_adapters::ssh_transport::RusshTransport;

use crate::diagnostics::export_diagnostics;
use crate::sessions::{
    create_session, delete_folder, delete_session, duplicate_session, list_folders, list_sessions,
    save_folder, update_session,
};
use crate::sftp::{
    downloads_dir, sftp_chmod, sftp_home, sftp_list, sftp_mkdir, sftp_remove, sftp_rename,
};
use crate::terminal::{AppState, close_tab, connect, disconnect, terminal_resize, terminal_write};
use crate::transfers::{
    cancel_transfer, clear_transfers, enqueue_download, enqueue_upload, list_transfers,
    transfer_worker,
};

fn main() {
    if let Err(error) = tauri::Builder::default()
        .setup(|app| {
            match app.path().app_log_dir() {
                Ok(log_dir) => match logging::init(&log_dir) {
                    Ok(guard) => {
                        app.manage(guard);
                    }
                    Err(error) => eprintln!("warning: file logging disabled: {error}"),
                },
                Err(error) => eprintln!("warning: log directory unavailable: {error}"),
            }
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let known_hosts_path = data_dir.join("known_hosts");
            let repository = SqliteSessionRepository::open(&data_dir.join("sessions.db"))?;
            let state = AppState::new(RusshTransport::new(), known_hosts_path, repository);
            let transfers = Arc::clone(&state.transfers);
            let notify = Arc::clone(&state.notify);
            let fs = state.fs.clone();
            app.manage(state);
            tauri::async_runtime::spawn(transfer_worker(
                transfers,
                notify,
                fs,
                Arc::new(app.handle().clone()),
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
            delete_folder,
            enqueue_download,
            enqueue_upload,
            cancel_transfer,
            clear_transfers,
            list_transfers,
            sftp_home,
            sftp_list,
            sftp_mkdir,
            sftp_rename,
            sftp_remove,
            sftp_chmod,
            downloads_dir,
            export_diagnostics
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
        std::process::exit(1);
    }
}
