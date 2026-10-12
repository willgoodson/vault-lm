//! Vault layout: Notebook (folder under the per-vault root) → Section (subfolder) → Topic (.md file).
//! Sources live beside their owner: <nb>/sources/, <section>/sources/, <dir>/sources/<Topic>/ (topic sidecar).
use crate::util::{self, basename, dirname, join, join_rel, natural_cmp, stem, strip_md};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const SKIP_DIRS: [&str; 2] = ["_studio", "_briefs"];
pub fn non_content(name: &str) -> bool { SKIP_DIRS.contains(&name) || name == "sources" || name == "attachments" }

#[derive(Serialize, Clone, Debug)]
pub struct Item { pub name: String, pub path: String }

#[derive(Serialize, Clone, Debug)]
pub struct Topic { pub name: String, pub path: String, pub sources: Vec<Item> }

#[derive(Serialize, Clone, Debug)]
pub struct Section {
    pub name: String, pub path: String, pub topics: Vec<Topic>, pub sources: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")] pub meta: Option<Value>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Notebook {
    pub name: String, pub path: String, pub topics: Vec<Topic>, pub sections: Vec<Section>, pub sources: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")] pub meta: Option<Value>,
}
impl Notebook {
    pub fn topic_count(&self) -> usize { self.topics.len() + self.sections.iter().map(|s| s.topics.len()).sum::<usize>() }
    pub fn all_topics(&self) -> impl Iterator<Item = &Topic> { self.topics.iter().chain(self.sections.iter().flat_map(|s| s.topics.iter())) }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Settings { #[serde(default)] pub root: String }

pub fn get_settings(data: &Path) -> Settings {
    serde_json::from_value(util::read_json(&data.join("settings.json"))).unwrap_or_default()
}
pub fn save_settings(data: &Path, root: &str) -> std::io::Result<Settings> {
    let s = Settings { root: root.trim_matches('/').to_string() };
    util::write_atomic(&data.join("settings.json"), serde_json::to_string_pretty(&s)?.as_bytes())?;
    Ok(s)
}

struct Entry { name: String, is_dir: bool }
fn entries(root: &Path, rel: &str) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(join_rel(root, rel)).into_iter().flatten().flatten()
        .filter_map(|e| { let name = e.file_name().to_string_lossy().to_string(); (!name.starts_with('.')).then(|| Entry { is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false), name }) })
        .collect();
    v.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    v
}

pub fn top_folders(root: &Path) -> Vec<String> { entries(root, "").into_iter().filter(|e| e.is_dir).map(|e| e.name).collect() }

fn md_files(root: &Path, rel: &str, recursive: bool, out: &mut Vec<String>) {
    for e in entries(root, rel) {
        let p = join(rel, &e.name);
        if e.is_dir { if recursive && !non_content(&e.name) { md_files(root, &p, true, out); } }
        else if e.name.ends_with(".md") { out.push(p); }
    }
}
fn md_list(root: &Path, rel: &str, recursive: bool) -> Vec<String> { let mut v = vec![]; md_files(root, rel, recursive, &mut v); v }
fn item(p: &str) -> Item { Item { name: strip_md(basename(p)).to_string(), path: p.to_string() } }

/// Files directly in a sources/ folder (topic sidecars are its subfolders).
fn sources_in(root: &Path, folder: &str) -> Vec<Item> { md_list(root, &join(folder, "sources"), false).iter().map(|p| item(p)).collect() }

pub fn topic_sources_dir(topic: &str) -> String { join(&join(dirname(topic), "sources"), stem(basename(topic))) }

fn topics_in(root: &Path, folder: &str, recursive: bool) -> Vec<Topic> {
    md_list(root, folder, recursive).into_iter().map(|p| {
        let rel = p.strip_prefix(&format!("{folder}/")).unwrap_or(&p);
        Topic { name: strip_md(rel).to_string(), sources: md_list(root, &topic_sources_dir(&p), true).iter().map(|s| item(s)).collect(), path: p }
    }).collect()
}

pub fn list_notebooks(root: &Path, data: &Path) -> Vec<Notebook> {
    let settings = get_settings(data);
    let nb_root = settings.root;
    let mut out = vec![];
    for e in entries(root, &nb_root) {
        if !e.is_dir || SKIP_DIRS.contains(&e.name.as_str()) { continue; }
        let nb = join(&nb_root, &e.name);
        let sections = entries(root, &nb).into_iter().filter(|s| s.is_dir && !non_content(&s.name)).map(|s| {
            let sp = join(&nb, &s.name);
            Section { topics: topics_in(root, &sp, true), sources: sources_in(root, &sp), name: s.name, path: sp, meta: None }
        }).collect();
        out.push(Notebook { name: e.name, topics: topics_in(root, &nb, false), sections, sources: sources_in(root, &nb), path: nb, meta: None });
    }
    out
}

/// Scope = { notebook, section?, topic? } as sent by the UI.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Scope {
    #[serde(default)] pub notebook: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub mode: Option<String>,
}
impl Scope {
    /// Folder that a scope's generated output lives in.
    pub fn base(&self) -> String {
        if let Some(t) = &self.topic { return dirname(t).to_string(); }
        self.section.clone().unwrap_or_else(|| self.notebook.clone())
    }
    /// Where "Add source" saves for this scope.
    pub fn sources_dir(&self) -> String {
        match &self.topic { Some(t) => topic_sources_dir(t), None => join(self.section.as_deref().unwrap_or(&self.notebook), "sources") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_shape() {
        let d = tempfile::tempdir().unwrap(); let data = tempfile::tempdir().unwrap();
        let w = |p: &str| { let f = d.path().join(p); std::fs::create_dir_all(f.parent().unwrap()).unwrap(); std::fs::write(f, "# x").unwrap(); };
        w("P/Goals.md"); w("P/Shed/Shed.md"); w("P/Shed/sources/Plans.md"); w("P/Shed/sources/Shed/Permit.md"); w("P/_studio/Overview.md"); w("P/Task 10.md"); w("P/Task 2.md");
        let nbs = list_notebooks(d.path(), data.path());
        assert_eq!(nbs.len(), 1);
        let p = &nbs[0];
        assert_eq!(p.topics.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Goals", "Task 2", "Task 10"]);
        assert_eq!(p.sections[0].name, "Shed");
        assert_eq!(p.sections[0].sources[0].name, "Plans");
        assert_eq!(p.sections[0].topics[0].sources[0].path, "P/Shed/sources/Shed/Permit.md");
        assert_eq!(topic_sources_dir("P/Shed/Shed.md"), "P/Shed/sources/Shed");
    }
}
