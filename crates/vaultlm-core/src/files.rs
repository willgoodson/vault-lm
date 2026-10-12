//! All vault filesystem access. Vaults stay Obsidian-compatible: plain Markdown, nothing is ever
//! written to .obsidian/, and deletes go to .trash/ (Obsidian's "vault trash").
use crate::error::{ApiError, Result};
use crate::util::{self, basename, clean_name, dirname, ext, join, join_rel, mtime_ms, stem};
use serde_json::json;
use std::path::{Component, Path, PathBuf};

pub struct Files<'a> { pub root: &'a Path }

impl<'a> Files<'a> {
    pub fn new(root: &'a Path) -> Self { Files { root } }

    /// Resolve a vault-relative path, refusing anything outside the vault or inside .obsidian/.
    pub fn safe(&self, rel: &str) -> Result<PathBuf> {
        let mut parts: Vec<&str> = vec![];
        for c in Path::new(rel).components() {
            match c {
                Component::Normal(s) => parts.push(s.to_str().ok_or_else(|| ApiError::bad_request("Invalid path"))?),
                Component::ParentDir => { if parts.pop().is_none() { return Err(ApiError::forbidden("Outside vault")); } }
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) => return Err(ApiError::forbidden("Outside vault")),
            }
        }
        if parts.first() == Some(&".obsidian") { return Err(ApiError::forbidden("Vault-LM never touches .obsidian/")); }
        Ok(join_rel(self.root, &parts.join("/")))
    }

    pub fn exists(&self, rel: &str) -> bool { self.safe(rel).map(|p| p.exists()).unwrap_or(false) }

    pub fn read(&self, rel: &str) -> Result<(String, f64)> {
        let full = self.safe(rel)?;
        let content = std::fs::read_to_string(&full)?;
        Ok((content, mtime_ms(&full).unwrap_or(0.0)))
    }

    pub fn write(&self, rel: &str, content: &str) -> Result<f64> {
        let full = self.safe(rel)?;
        if let Some(p) = full.parent() { std::fs::create_dir_all(p)?; }
        std::fs::write(&full, content)?;
        Ok(mtime_ms(&full).unwrap_or(0.0))
    }

    /// Editor save with optimistic concurrency: refuse if the file changed on disk since it was loaded.
    pub fn save(&self, rel: &str, content: &str, base_mtime: Option<f64>) -> Result<f64> {
        let full = self.safe(rel)?;
        if let (Some(current), Some(base)) = (mtime_ms(&full), base_mtime) {
            if (current - base).abs() > 1.0 {
                let disk = std::fs::read_to_string(&full).unwrap_or_default();
                return Err(ApiError::conflict("Changed outside Vault-LM").with_extra(json!({ "current": disk, "mtime": current })));
            }
        }
        self.write(rel, content)
    }

    /// "Name.md", "Name 1.md", … — never overwrite.
    fn free_name(dir: &Path, base: &str, ext: &str) -> String {
        for i in 0.. {
            let name = if i == 0 { format!("{base}{ext}") } else { format!("{base} {i}{ext}") };
            if !dir.join(&name).exists() { return name; }
        }
        unreachable!()
    }

    pub fn create_note(&self, folder: &str, name: &str, content: Option<&str>) -> Result<String> {
        let name = clean_name(name);
        if name.is_empty() { return Err(ApiError::bad_request("Name required")); }
        let dir = self.safe(if folder.is_empty() { "." } else { folder })?;
        std::fs::create_dir_all(&dir)?;
        let file = Self::free_name(&dir, &name, ".md");
        let rel = join(folder, &file);
        let body = content.map(String::from).unwrap_or_else(|| format!("# {}\n\n", stem(&file)));
        self.write(&rel, &body)?;
        Ok(rel)
    }

    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String> {
        let name = clean_name(name);
        if name.is_empty() { return Err(ApiError::bad_request("Name required")); }
        let rel = join(parent, &name);
        let full = self.safe(&rel)?;
        if full.exists() { return Err(ApiError::conflict(format!("“{name}” already exists"))); }
        std::fs::create_dir_all(&full)?;
        Ok(rel)
    }

    pub fn trash(&self, rel: &str) -> Result<String> {
        let full = self.safe(rel)?;
        if full == self.root { return Err(ApiError::bad_request("Cannot delete the vault")); }
        let trash = self.root.join(".trash");
        std::fs::create_dir_all(&trash)?;
        let name = basename(rel);
        let e = ext(name);
        let dot_ext = if e.is_empty() || full.is_dir() { String::new() } else { format!(".{e}") };
        let base = if dot_ext.is_empty() { name } else { stem(name) };
        let dest_name = Self::free_name(&trash, base, &dot_ext);
        std::fs::rename(&full, trash.join(&dest_name))?;
        Ok(format!(".trash/{dest_name}"))
    }

    pub fn move_to(&self, rel: &str, new_rel: &str) -> Result<()> {
        let (from, to) = (self.safe(rel)?, self.safe(new_rel)?);
        if to.exists() { return Err(ApiError::conflict(format!("“{}” already exists", basename(new_rel)))); }
        if let Some(p) = to.parent() { std::fs::create_dir_all(p)?; }
        std::fs::rename(from, to)?;
        Ok(())
    }

    /// Remove a directory only if it's empty (used to tidy sources/ and attachments/ folders).
    pub fn remove_if_empty(&self, rel: &str) {
        if let Ok(p) = self.safe(rel) { let _ = std::fs::remove_dir(p); }
    }

    /// Obsidian's attachment setting (read-only). Without one, files go in "attachments" next to the note.
    pub fn attachment_dir(&self, note_rel: &str) -> String {
        let cfg = util::read_json(&self.root.join(".obsidian/app.json"));
        let note_dir = dirname(note_rel);
        match cfg["attachmentFolderPath"].as_str() {
            None => join(note_dir, "attachments"),
            Some("/") | Some("") => String::new(),
            Some("./") => note_dir.to_string(),
            Some(s) if s.starts_with("./") => join(note_dir, &s[2..]),
            Some(s) => s.trim_matches('/').to_string(),
        }
    }

    pub fn save_attachment(&self, note_rel: &str, filename: &str, bytes: &[u8]) -> Result<(String, String)> {
        let dir = self.attachment_dir(note_rel);
        let dir_full = self.safe(if dir.is_empty() { "." } else { &dir })?;
        std::fs::create_dir_all(&dir_full)?;
        let e = ext(filename);
        let dot_ext = if e.is_empty() { ".png".to_string() } else { format!(".{e}") };
        let mut base = clean_name(stem(filename));
        if base.is_empty() { base = format!("Pasted image {}", chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S")); }
        let name = Self::free_name(&dir_full, &base, &dot_ext);
        std::fs::write(dir_full.join(&name), bytes)?;
        Ok((join(&dir, &name), name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safety() {
        let d = tempfile::tempdir().unwrap();
        let f = Files::new(d.path());
        assert!(f.safe("a/b.md").is_ok());
        assert_eq!(f.safe("../x").unwrap_err().status, 403);
        assert_eq!(f.safe("/etc/passwd").unwrap_err().status, 403);
        assert_eq!(f.safe(".obsidian/app.json").unwrap_err().status, 403);
        assert!(f.safe("a/../b.md").is_ok());
    }
    #[test]
    fn create_trash_conflict() {
        let d = tempfile::tempdir().unwrap();
        let f = Files::new(d.path());
        let a = f.create_note("NB", "Topic", None).unwrap();
        let b = f.create_note("NB", "Topic", None).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("NB/Topic.md", "NB/Topic 1.md"));
        let (_, m) = f.read(&a).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        f.write(&a, "changed elsewhere").unwrap();
        assert_eq!(f.save(&a, "mine", Some(m)).unwrap_err().status, 409);
        assert_eq!(f.trash(&a).unwrap(), ".trash/Topic.md");
        assert_eq!(f.trash(&b).unwrap(), ".trash/Topic 1.md");
    }
}
