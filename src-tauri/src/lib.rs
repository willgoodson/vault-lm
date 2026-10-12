//! Vault-LM desktop shell: hosts the Vue UI and bridges it to `vaultlm-core` over Tauri IPC.
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::menu::{AboutMetadataBuilder, Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{Emitter, Manager, State};
use vaultlm_core::Core;

/// Single bridge for the UI's route-style API. Errors come back as { status, message, ...extra }
/// so the existing frontend error handling (409 conflicts, toasts) works unchanged.
#[tauri::command]
async fn api(app: tauri::AppHandle, core: State<'_, Arc<Core>>, method: String, path: String, query: Option<Value>, body: Option<Value>) -> Result<Value, Value> {
    let res = core.api(&method, &path, query.unwrap_or(Value::Null), body.unwrap_or(Value::Null)).await;
    if path.starts_with("/api/vaults/") { allow_vault_assets(&app, &core); }
    res.map_err(|e| {
        let mut v = json!({ "status": e.status, "message": e.message });
        if let (Some(extra), Some(obj)) = (e.extra, v.as_object_mut()) {
            if let Some(x) = extra.as_object() { obj.extend(x.clone()); }
        }
        v
    })
}

/// Streaming endpoints (chat): each event object ({token}, {sources}, {followups}) goes over a Channel.
#[tauri::command]
async fn stream(core: State<'_, Arc<Core>>, path: String, body: Option<Value>, on_event: tauri::ipc::Channel<Value>) -> Result<(), Value> {
    let send = move |v: Value| { let _ = on_event.send(v); };
    core.stream(&path, body.unwrap_or(Value::Null), &send).await.map_err(|e| json!({ "status": e.status, "message": e.message }))
}

/// Let the WebView load images/audio from the open vault (and nothing else) via the asset protocol.
fn allow_vault_assets(app: &tauri::AppHandle, core: &Core) {
    if let Ok(v) = core.vault() { let _ = app.asset_protocol_scope().allow_directory(&v.path, true); }
}

/// Native menus. The Edit menu matters on macOS: copy/paste/undo only reach the WebView through it.
/// ⌘W closes the current tab (handled by the UI); closing the window is ⇧⌘W.
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str, label: &str, accel: Option<&str>| {
        let b = MenuItemBuilder::with_id(id, label);
        match accel { Some(a) => b.accelerator(a).build(app), None => b.build(app) }
    };
    let about = AboutMetadataBuilder::new()
        .name(Some("Vault-LM")).version(Some(env!("CARGO_PKG_VERSION")))
        .comments(Some("Your notes and a study partner, all on your computer."))
        .credits(Some("MIT License. Built with Tauri, LanceDB, PDFium, ocrs, CodeMirror and Vue. Help → Third-Party Licenses lists every included library."))
        .build();
    let app_menu = SubmenuBuilder::new(app, "Vault-LM")
        .item(&PredefinedMenuItem::about(app, Some("About Vault-LM"), Some(about))?)
        .separator()
        .item(&item("models", "Models…", Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item("open-vault", "Open Vault…", Some("CmdOrCtrl+O"))?)
        .item(&item("new-vault", "New Vault…", Some("CmdOrCtrl+Shift+N"))?)
        .separator()
        .item(&item("close-tab", "Close Tab", Some("CmdOrCtrl+W"))?)
        .item(&PredefinedMenuItem::close_window(app, Some("Close Window"))?)
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?).item(&PredefinedMenuItem::redo(app, None)?).separator()
        .item(&PredefinedMenuItem::cut(app, None)?).item(&PredefinedMenuItem::copy(app, None)?).item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item("search", "Search…", Some("CmdOrCtrl+K"))?)
        .item(&item("review", "Review Dashboard", Some("CmdOrCtrl+Shift+R"))?)
        .item(&item("toggle-studio", "Toggle Studio", Some("CmdOrCtrl+."))?)
        .item(&item("toggle-theme", "Toggle Dark Mode", Some("CmdOrCtrl+Shift+D"))?)
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?).item(&PredefinedMenuItem::maximize(app, Some("Zoom"))?)
        .build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .item(&item("models", "Model Settings…", None)?)
        .item(&item("data-folder", "Show App Data Folder", None)?)
        .separator()
        .item(&item("licenses", "Third-Party Licenses", None)?)
        .build()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch focuses the existing window instead of starting a competing instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") { let _ = w.unminimize(); let _ = w.set_focus(); }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .menu(build_menu)
        // Menu items the UI handles (open vault, toggle studio, …) are forwarded as a "menu" event.
        .on_menu_event(|app, ev| {
            let id = ev.id().0.as_str();
            match id {
                "licenses" => {
                    if let Ok(f) = app.path().resolve("THIRD-PARTY-NOTICES.txt", tauri::path::BaseDirectory::Resource) {
                        let _ = tauri_plugin_opener::open_path(f.to_string_lossy().to_string(), None::<&str>);
                    }
                }
                "data-folder" => { let _ = tauri_plugin_opener::open_path(vaultlm_core::paths::data_dir().to_string_lossy().to_string(), None::<&str>); }
                _ => { let _ = app.emit("menu", json!({ "id": id })); }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let emit: vaultlm_core::Emitter = Arc::new(move |name: &str, payload: Value| { let _ = handle.emit(name, payload); });
            // PDFium ships as a bundled resource on every platform.
            if std::env::var("VAULTLM_PDFIUM").is_err() {
                if let Ok(dir) = app.path().resource_dir() { std::env::set_var("VAULTLM_PDFIUM", dir.join("pdfium")); }
            }
            // Created inside the async runtime so background indexing/overviews have an executor.
            let core = tauri::async_runtime::block_on(async move { Core::new(vaultlm_core::paths::data_dir(), emit) });
            allow_vault_assets(app.handle(), &core);
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api, stream])
        .run(tauri::generate_context!())
        .expect("error while running Vault-LM");
}
