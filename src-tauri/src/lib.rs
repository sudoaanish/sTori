mod db;
mod downloads;
mod error;
mod models;
mod scanner;
mod server;

use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("stori=info".parse().unwrap()),
        )
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = std::env::var_os("STORI_DATA_DIR").map(PathBuf::from).unwrap_or(app.path().app_data_dir()?);
            let database =
                db::Database::open(&data_dir.join("stori.db")).map_err(|e| e.to_string())?;
            let managed_library = std::env::var_os("STORI_MANAGED_LIBRARY_DIR").map(PathBuf::from).unwrap_or(app.path().download_dir()?.join("sTori Books"));
            let state = server::ServerState::new(database, managed_library);
            let resource_dist = app.path().resource_dir()?.join("dist");
            let dev_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("dist");
            let dist = if resource_dist.join("index.html").exists() {
                resource_dist
            } else {
                dev_dist
            };
            let listener = match std::net::TcpListener::bind(("0.0.0.0", server::PORT)) {
                Ok(listener) => listener,
                Err(error) => {
                    let detail = port_conflict_detail(server::PORT);
                    app.dialog()
                        .message(format!("sTori could not start its local server on port {}.\n\n{}\n\nClose the conflicting application, then reopen sTori.\n\nTechnical detail: {error}", server::PORT, detail))
                        .title("sTori server could not start")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(format!("Port {} unavailable: {error}", server::PORT).into());
                }
            };
            tauri::async_runtime::spawn(async move {
                if let Err(error) = server::run_with_std_listener(state, dist, listener).await {
                    tracing::error!("sTori server stopped: {error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running sTori");
}

#[cfg(windows)]
fn port_conflict_detail(port: u16) -> String {
    let output = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output();
    let pid = output
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| {
            text.lines().find_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                (fields.len() >= 5
                    && fields[1].ends_with(&format!(":{port}"))
                    && fields[3].eq_ignore_ascii_case("LISTENING"))
                .then(|| fields[4].to_string())
            })
        });
    if let Some(pid) = pid {
        let app = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|text| {
                text.split(',')
                    .next()
                    .map(|value| value.trim_matches('"').to_string())
            })
            .filter(|value| !value.starts_with("INFO:"));
        return match app {
            Some(app) => format!("Port {port} is currently used by {app} (PID {pid})."),
            None => format!("Port {port} is currently used by PID {pid}."),
        };
    }
    format!("Port {port} is already in use by another application.")
}

#[cfg(not(windows))]
fn port_conflict_detail(port: u16) -> String {
    format!("Port {port} is already in use by another application.")
}
