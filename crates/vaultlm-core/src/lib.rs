//! Vault-LM core: everything the desktop app does, independent of the UI shell.
//! `Core::api(method, path, query, body)` mirrors the route contract the Vue UI already uses.
pub mod ai;
pub mod audio;
pub mod api;
pub mod error;
pub mod files;
pub mod index;
pub mod layout;
pub mod links;
pub mod models;
pub mod paths;
pub mod pdfmd;
pub mod progress;
pub mod srs;
pub mod retrieve;
pub mod sources;
pub mod store;
pub mod util;
pub mod vaults;

use error::{ApiError, Result};
use notify_debouncer_full::{new_debouncer, notify::RecursiveMode, DebounceEventResult, Debouncer, RecommendedCache};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use vaults::Vault;

/// Event sink provided by the shell (Tauri `emit`), e.g. ("tree", {}), ("file", {path, mtime}).
pub type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct Core {
    pub data_root: PathBuf,
    vault: RwLock<Option<Vault>>,
    pub(crate) links: Mutex<links::LinkIndex>,
    pub(crate) ui_lock: Mutex<()>,
    pub models: models::Models,
    /// Cached read handle on the current vault's search index (reopened after each sync).
    pub(crate) reader: Mutex<Option<Arc<index::SearchIndex>>>,
    pub(crate) indexing: tokio::sync::Mutex<()>,
    pub(crate) status: RwLock<Value>,
    pub(crate) overview_running: RwLock<Option<String>>,
    pub(crate) rt: Option<tokio::runtime::Handle>,
    pub(crate) jobs: Mutex<std::collections::HashMap<String, Value>>,
    emit: Emitter,
    watcher: Mutex<Option<Debouncer<notify::RecommendedWatcher, RecommendedCache>>>,
    pub(crate) me: Weak<Core>,
}

impl Core {
    pub fn new(data_root: PathBuf, emit: Emitter) -> Arc<Core> {
        let _ = std::fs::create_dir_all(&data_root);
        if let Some(from) = paths::import_legacy(&data_root) { eprintln!("Imported Vault-LM data from {}", from.display()); }
        let models = models::Models::new(&data_root);
        let core = Arc::new_cyclic(|me| Core {
            data_root, vault: RwLock::new(None), links: Mutex::new(Default::default()),
            ui_lock: Mutex::new(()), models, reader: Mutex::new(None), indexing: tokio::sync::Mutex::new(()),
            status: RwLock::new(json!({ "running": false, "done": 0, "total": 0, "chunks": 0 })), overview_running: RwLock::new(None),
            rt: tokio::runtime::Handle::try_current().ok(), jobs: Mutex::new(Default::default()), emit, watcher: Mutex::new(None), me: me.clone(),
        });
        if let Some(p) = vaults::initial_vault(&core.data_root) { let _ = core.open_vault(&p); }
        core
    }

    pub fn emit(&self, name: &str, payload: Value) { (self.emit)(name, payload) }

    pub fn vault(&self) -> Result<Vault> {
        self.vault.read().clone().ok_or_else(|| ApiError::new(412, "No vault open yet. Choose or create one."))
    }

    pub fn open_vault(&self, p: &Path) -> Result<Vault> {
        if !p.is_dir() { return Err(ApiError::bad_request(format!("Not a folder: {}", p.display()))); }
        let v = Vault::at(&self.data_root, p);
        std::fs::create_dir_all(&v.data)?;
        vaults::remember(&self.data_root, &v)?;
        *self.vault.write() = Some(v.clone());
        *self.reader.lock() = None;
        self.links.lock().build(&v.path);
        self.watch(&v.path);
        self.emit("tree", json!({}));
        self.reindex();
        Ok(v)
    }

    /// Watch the vault: per-file events for open editors, then a refreshed link index + tree.
    fn watch(&self, root: &Path) {
        let me = self.me.clone();
        let root_owned = root.to_path_buf();
        let debouncer = new_debouncer(Duration::from_millis(500), None, move |res: DebounceEventResult| {
            let Some(core) = me.upgrade() else { return };
            let Ok(events) = res else { return };
            let (mut touched, mut notes) = (false, false);
            for ev in events {
                for p in &ev.paths {
                    let rel = util::rel_of(&root_owned, p);
                    if rel.split('/').any(|s| s.starts_with('.')) { continue; } // .obsidian, .trash, iCloud placeholders
                    touched = true;
                    notes |= rel.ends_with(".md");
                    core.emit("file", json!({ "event": format!("{:?}", ev.kind), "path": rel, "mtime": util::mtime_ms(p) }));
                }
            }
            if touched {
                if let Ok(v) = core.vault() { core.links.lock().build(&v.path); }
                core.emit("tree", json!({}));
                if notes { core.reindex(); }
            }
        });
        if let Ok(mut d) = debouncer {
            if d.watch(root, RecursiveMode::Recursive).is_ok() { *self.watcher.lock() = Some(d); }
        }
    }
}
