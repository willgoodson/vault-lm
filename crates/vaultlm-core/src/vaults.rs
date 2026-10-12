//! The current vault, the list of known vaults, and per-vault app data folders.
use crate::{paths, util};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct Vault {
    pub name: String,
    pub path: PathBuf,
    /// Vault-LM's data for this vault: <data>/vaults/<slug>/
    pub data: PathBuf,
}

impl Vault {
    pub fn at(data_root: &Path, p: &Path) -> Self {
        let path = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let data = data_root.join("vaults").join(util::slug(&path.to_string_lossy()));
        Vault { name, path, data }
    }
}

fn state_file(data_root: &Path) -> PathBuf { data_root.join("state.json") }

/// The vault to open at launch: last used, else ~/Documents/Notebooks.
pub fn initial_vault(data_root: &Path) -> Option<PathBuf> {
    if let Ok(v) = std::env::var("VAULT") { return Some(PathBuf::from(v)); }
    let s = util::read_json(&state_file(data_root));
    s["vault"].as_str().map(PathBuf::from).filter(|p| p.is_dir())
}

pub fn remember(data_root: &Path, v: &Vault) -> std::io::Result<()> {
    let s = util::read_json(&state_file(data_root));
    let mut list: Vec<String> = vec![v.path.to_string_lossy().to_string()];
    for p in s["vaults"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
        if !list.iter().any(|x| x == p) { list.push(p.to_string()); }
    }
    let out = json!({ "vault": v.path, "vaults": list });
    util::write_atomic(&state_file(data_root), serde_json::to_string_pretty(&out)?.as_bytes())
}

/// Vaults Vault-LM has opened, plus ones Obsidian knows about.
pub fn known(data_root: &Path, current: Option<&Vault>) -> Vec<Value> {
    let s = util::read_json(&state_file(data_root));
    let ours: Vec<String> = s["vaults"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    let obsidian: Vec<String> = paths::obsidian_registry()
        .map(|f| util::read_json(&f))
        .and_then(|c| c["vaults"].as_object().cloned())
        .map(|m| m.values().filter_map(|v| v["path"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    let mut out: Vec<Value> = vec![];
    let mut seen = std::collections::HashSet::new();
    if let Some(c) = current {
        let p = c.path.to_string_lossy().to_string();
        if !ours.contains(&p) && !obsidian.contains(&p) { seen.insert(p.clone()); out.push(json!({ "name": c.name, "path": p, "current": true, "indexed": c.data.join("lancedb").exists() })); }
    }
    for p in ours.iter().chain(obsidian.iter()) {
        if !seen.insert(p.clone()) { continue; }
        let name = Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let indexed = data_root.join("vaults").join(util::slug(p)).join("lancedb").exists();
        out.push(json!({
            "name": name, "path": p, "fromObsidian": !ours.contains(p),
            "current": current.map(|c| c.path.to_string_lossy() == p.as_str()).unwrap_or(false),
            "indexed": indexed,
        }));
    }
    out
}
