mod db;
mod downloads;
mod error;
mod models;
mod scanner;
mod server;

use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = std::env::var_os("STORI_DATA_DIR").map(PathBuf::from).unwrap_or(app.path().app_data_dir()?);
            let database =
                db::Database::open(&data_dir.join("stori.db")).map_err(|e| e.to_string())?;
            let managed_library = std::env::var_os("STORI_MANAGED_LIBRARY_DIR").map(PathBuf::from).unwrap_or(app.path().download_dir()?.join("sTori Books"));
            let state = server::ServerState::new(database, managed_library, data_dir.join("cover-cache"));
            let resource_dir = app.path().resource_dir()?;
            let dev_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("dist");
            let dist = [
                resource_dir.join("dist"),
                // NSIS packages Tauri resources under `_up_`. Keep the normal
                // resource layout first for development and future platform bundles.
                resource_dir.join("_up_").join("dist"),
                dev_dist,
            ]
            .into_iter()
            .find(|candidate| candidate.join("index.html").is_file())
            .ok_or("sTori web assets are missing from this installation")?;
            tracing::info!(path = %dist.display(), "Serving sTori web assets");
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
            let show_item = MenuItem::with_id(app, "show", "Show sTori", true, None::<&str>)?;
            let hide_item = MenuItem::with_id(app, "hide", "Hide to system tray", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit sTori", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;
            let tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().ok_or("sTori tray icon is unavailable")?.clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("sTori — your personal reading room")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            app.manage(tray);
            if std::env::args().any(|argument| argument == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_autostart, set_autostart])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running sTori");
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn get_autostart() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let output = hidden_windows_command("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "sTori",
            ])
            .output()
            .map_err(|error| format!("Could not read the Windows startup setting: {error}"))?;
        Ok(output.status.success())
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = if enabled {
            let executable = std::env::current_exe()
                .map_err(|error| format!("Could not locate sTori: {error}"))?;
            hidden_windows_command("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "sTori",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\" --minimized", executable.display()),
                    "/f",
                ])
                .status()
        } else {
            hidden_windows_command("reg")
                .args([
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "sTori",
                    "/f",
                ])
                .status()
        }
        .map_err(|error| format!("Could not change the Windows startup setting: {error}"))?;
        if enabled && !status.success() {
            return Err("Windows did not accept the startup setting.".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("Start with Windows is only available on Windows.".into())
    }
}

#[cfg(windows)]
fn hidden_windows_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW prevents short-lived console tools such as reg.exe
    // from flashing a CMD window behind the desktop app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(windows)]
fn port_conflict_detail(port: u16) -> String {
    let output = hidden_windows_command("netstat")
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
        let app = hidden_windows_command("tasklist")
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
