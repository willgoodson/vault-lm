//! Where Vault-LM keeps its own data (never inside a vault): the OS app-data folder,
//! overridable with VAULTLM_DATA (used by tests and for importing the old Node data folder).
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("VAULTLM_DATA") { return PathBuf::from(p); }
    directories::ProjectDirs::from("com", "vaultlm", "Vault-LM")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".vaultlm-data"))
}

/// Obsidian's registry of vaults (read-only; Vault-LM never writes it).
pub fn obsidian_registry() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    // macOS: ~/Library/Application Support/obsidian, Windows: %APPDATA%\obsidian, Linux: ~/.config/obsidian
    Some(base.config_dir().join("obsidian").join("obsidian.json"))
}

/// One-time import of the previous (Node) version's data folder: per-vault indexes, decks, details,
/// progress, chats and the vault list. Runs only when the desktop data folder is still empty.
pub fn import_legacy(data_root: &std::path::Path) -> Option<std::path::PathBuf> {
    if data_root.join("state.json").exists() { return None; }
    let legacy = std::env::var("VAULTLM_LEGACY_DATA").map(PathBuf::from).ok().or_else(|| {
        directories::BaseDirs::new().map(|b| b.home_dir().join("projects/vault-lm/data"))
    })?;
    if !legacy.join("state.json").exists() || legacy == data_root { return None; }
    fn copy(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            let (src, dst) = (e.path(), to.join(e.file_name()));
            if e.file_type()?.is_dir() { copy(&src, &dst)?; } else { std::fs::copy(&src, &dst)?; }
        }
        Ok(())
    }
    for name in ["state.json", "models.json"] { let _ = std::fs::copy(legacy.join(name), data_root.join(name)); }
    if legacy.join("vaults").is_dir() { copy(&legacy.join("vaults"), &data_root.join("vaults")).ok()?; }
    Some(legacy)
}
