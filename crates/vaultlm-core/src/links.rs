//! Link index: Obsidian-style resolution of [[wikilinks]], ![[embeds]] and [md](links), plus tags and
//! frontmatter. Rebuilt incrementally (by mtime); used for backlinks, graph, search and rename rewrites.
use crate::util::{basename, dirname, join, join_rel, mtime_ms, rel_of, strip_md};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WIKI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(!?)\[\[([^\]|#^]*)(#[^\]|^]*)?(\^[^\]|]*)?(\|[^\]]*)?\]\]").unwrap());
static MDLINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]\(([^)\s]+?\.md)(#[^)]*)?\)").unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[\s(])#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```.*?```").unwrap());
static INLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`\n]*`").unwrap());
static HEADING_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#{1,6}\s").unwrap());
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(#{1,6})\s+(.+?)\s*#*\s*$").unwrap());
static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\A---\r?\n(.*?)\r?\n---(?:\r?\n|\z)").unwrap());
// encodeURIComponent's unreserved set: A-Z a-z 0-9 - _ . ! ~ * ' ( )
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');

#[derive(Clone, Debug, Serialize)]
pub struct Link { pub target: String, pub heading: String, pub embed: bool, pub alias: String, pub md: bool }

#[derive(Clone, Debug)]
pub struct FileInfo { pub mtime: f64, pub links: Vec<Link>, pub tags: Vec<String>, pub fm: Value, pub headings: Vec<(usize, String)> }

pub fn strip_code(s: &str) -> String { INLINE.replace_all(&FENCE.replace_all(s, ""), "").to_string() }

/// Split YAML frontmatter from the body. Invalid YAML yields an empty object (like gray-matter's tolerance).
pub fn split_frontmatter(content: &str) -> (Value, &str) {
    if let Some(m) = FRONTMATTER.captures(content) {
        let yaml = m.get(1).map(|g| g.as_str()).unwrap_or("");
        let fm = serde_yaml::from_str::<serde_yaml::Value>(yaml).ok().and_then(|y| serde_json::to_value(y).ok()).filter(|v| v.is_object()).unwrap_or_else(|| json!({}));
        return (fm, &content[m.get(0).unwrap().end()..]);
    }
    (json!({}), content)
}

fn is_external(t: &str) -> bool { ["http:", "https:", "mailto:", "obsidian:"].iter().any(|p| t.starts_with(p)) }
fn decode(s: &str) -> String { percent_decode_str(s).decode_utf8_lossy().to_string() }

pub fn parse(content: &str) -> FileInfo {
    let (fm, body) = split_frontmatter(content);
    let text = strip_code(body);
    let mut links = vec![];
    for c in WIKI.captures_iter(&text) {
        links.push(Link {
            target: c[2].trim().to_string(), heading: c.get(3).map(|m| m.as_str()[1..].to_string()).unwrap_or_default(),
            embed: !c[1].is_empty(), alias: c.get(5).map(|m| m.as_str()[1..].to_string()).unwrap_or_default(), md: false,
        });
    }
    for c in MDLINK.captures_iter(&text) {
        if is_external(&c[1]) { continue; }
        links.push(Link { target: decode(&c[1]), heading: c.get(2).map(|m| m.as_str()[1..].to_string()).unwrap_or_default(), embed: false, alias: String::new(), md: true });
    }
    let mut tags: Vec<String> = vec![];
    let no_headings = HEADING_MARK.replace_all(&text, "");
    for c in TAG.captures_iter(&no_headings) { let t = c[1].to_lowercase(); if !tags.contains(&t) { tags.push(t); } }
    let fm_tags: Vec<String> = match &fm["tags"] {
        Value::Array(a) => a.iter().filter_map(|t| t.as_str().map(String::from)).collect(),
        Value::String(s) => vec![s.clone()],
        _ => vec![],
    };
    for t in fm_tags { let t = t.trim_start_matches('#').to_lowercase(); if !t.is_empty() && !tags.contains(&t) { tags.push(t); } }
    let headings = HEADING.captures_iter(&FENCE.replace_all(body, "")).map(|c| (c[1].len(), c[2].trim().to_string())).collect();
    FileInfo { mtime: 0.0, links, tags, fm, headings }
}

#[derive(Default)]
pub struct LinkIndex {
    key: PathBuf,
    pub files: BTreeMap<String, FileInfo>,
    pub all: Vec<String>,
    all_set: HashSet<String>,
}

#[derive(Serialize)]
pub struct LinksOf { pub links: Vec<String>, pub embeds: Vec<String>, pub unresolved: Vec<String> }

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') { continue; }
        let p = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { walk(root, &p, out); } else { out.push(rel_of(root, &p)); }
    }
}

impl LinkIndex {
    pub fn build(&mut self, root: &Path) {
        if self.key != root { self.files.clear(); self.key = root.to_path_buf(); }
        let mut all = vec![];
        walk(root, root, &mut all);
        all.sort();
        let mut seen = HashSet::new();
        for rel in all.iter().filter(|r| r.ends_with(".md")) {
            seen.insert(rel.clone());
            let full = join_rel(root, rel);
            let Some(mtime) = mtime_ms(&full) else { continue };
            if self.files.get(rel).is_some_and(|f| f.mtime == mtime) { continue; }
            let content = std::fs::read_to_string(&full).unwrap_or_default();
            let mut info = parse(&content);
            info.mtime = mtime;
            self.files.insert(rel.clone(), info);
        }
        self.files.retain(|k, _| seen.contains(k));
        self.all_set = all.iter().cloned().collect();
        self.all = all;
    }

    /// Obsidian resolution: path-qualified match first, then unique basename, else shortest path.
    pub fn resolve(&self, target: &str, from: &str) -> Option<String> {
        if target.is_empty() { return (!from.is_empty()).then(|| from.to_string()); }
        let t = target.trim_start_matches('/');
        let has_other_ext = t.rsplit_once('.').is_some_and(|(_, e)| !e.is_empty() && e.chars().all(|c| c.is_alphanumeric() || c == '_') && e != "md");
        let candidates: Vec<String> = if has_other_ext { vec![t.to_string()] } else { vec![format!("{t}.md"), t.to_string()] };
        for c in &candidates {
            if self.all_set.contains(c) { return Some(c.clone()); }
            let rel = normalize(&join(dirname(from), c));
            if self.all_set.contains(&rel) { return Some(rel); }
        }
        let base = basename(&candidates[0]).to_lowercase();
        let suffix = format!("/{base}");
        let mut hits: Vec<&String> = self.all.iter().filter(|p| { let l = p.to_lowercase(); l.ends_with(&suffix) || l == base }).collect();
        if hits.is_empty() { return None; }
        if t.contains('/') {
            let want = candidates[0].to_lowercase();
            let mut tail: Vec<&String> = hits.iter().copied().filter(|p| p.to_lowercase().ends_with(&want)).collect();
            if !tail.is_empty() { tail.sort_by_key(|p| p.len()); return Some(tail[0].clone()); }
        }
        hits.sort_by(|a, b| a.matches('/').count().cmp(&b.matches('/').count()).then(a.len().cmp(&b.len())));
        Some(hits[0].clone())
    }

    fn resolved(&self, rel: &str) -> Vec<(Link, Option<String>)> {
        self.files.get(rel).map(|f| f.links.iter().map(|l| (l.clone(), self.resolve(&l.target, rel))).collect()).unwrap_or_default()
    }

    pub fn links_of(&self, rel: &str) -> LinksOf {
        let r = self.resolved(rel);
        let uniq = |it: Vec<String>| { let mut seen = HashSet::new(); it.into_iter().filter(|x| seen.insert(x.clone())).collect::<Vec<_>>() };
        LinksOf {
            links: uniq(r.iter().filter(|(l, p)| !l.embed && p.as_deref().is_some_and(|p| p != rel)).map(|(_, p)| p.clone().unwrap()).collect()),
            embeds: uniq(r.iter().filter(|(l, p)| l.embed && p.is_some()).map(|(_, p)| p.clone().unwrap()).collect()),
            unresolved: uniq(r.iter().filter(|(_, p)| p.is_none()).map(|(l, _)| l.target.clone()).collect()),
        }
    }

    pub fn backlinks_of(&self, rel: &str) -> Vec<String> {
        self.files.keys().filter(|p| p.as_str() != rel && self.resolved(p).iter().any(|(_, t)| t.as_deref() == Some(rel))).cloned().collect()
    }

    pub fn tags_of(&self, rel: &str) -> Vec<String> { self.files.get(rel).map(|f| f.tags.clone()).unwrap_or_default() }
    pub fn frontmatter_of(&self, rel: &str) -> Value { self.files.get(rel).map(|f| f.fm.clone()).unwrap_or_else(|| json!({})) }
    pub fn notes(&self) -> impl Iterator<Item = &String> { self.files.keys() }

    pub fn all_tags(&self) -> Vec<Value> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for f in self.files.values() { for t in &f.tags { *counts.entry(t).or_default() += 1; } }
        let mut v: Vec<(&str, usize)> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.into_iter().map(|(tag, count)| json!({ "tag": tag, "count": count })).collect()
    }

    pub fn graph(&self, prefix: Option<&str>) -> Value {
        let in_scope = |p: &str| p.ends_with(".md") && prefix.map_or(true, |pre| p.starts_with(&format!("{pre}/")));
        let nodes: Vec<&String> = self.files.keys().filter(|p| in_scope(p)).collect();
        let mut edges = vec![];
        for p in &nodes { for l in self.links_of(p).links { if in_scope(&l) { edges.push(json!({ "source": p, "target": l })); } } }
        json!({ "nodes": nodes, "edges": edges })
    }

    /// After moving files (old -> new pairs), rewrite links in every note that pointed at them.
    /// Keeps the link short (basename) when that still resolves uniquely, like Obsidian.
    pub fn rewrite_links(&mut self, root: &Path, moves: &[(String, String)]) -> Vec<String> {
        let before: HashMap<String, Vec<(Link, Option<String>)>> = self.files.keys().map(|p| (p.clone(), self.resolved(p))).collect();
        let map: HashMap<&str, &str> = moves.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        self.build(root);
        let mut changed = vec![];
        for (note, links) in &before {
            if !links.iter().any(|(_, p)| p.as_deref().is_some_and(|p| map.contains_key(p))) { continue; }
            let note_now = map.get(note.as_str()).map(|s| s.to_string()).unwrap_or_else(|| note.clone());
            let full = join_rel(root, &note_now);
            let Ok(content) = std::fs::read_to_string(&full) else { continue };
            let old_of = |target: &str| links.iter().find(|(l, _)| l.target == target).and_then(|(_, p)| p.clone());
            let name_for = |new_path: &str| {
                let short = basename(strip_md(new_path)).to_string();
                if self.resolve(&short, &note_now).as_deref() == Some(new_path) { short } else { strip_md(new_path).to_string() }
            };
            let next = WIKI.replace_all(&content, |c: &regex::Captures| {
                let old = old_of(c[2].trim());
                match old.as_deref().and_then(|o| map.get(o)) {
                    Some(newp) => format!("{}[[{}{}{}{}]]", &c[1], name_for(newp), c.get(3).map_or("", |m| m.as_str()), c.get(4).map_or("", |m| m.as_str()), c.get(5).map_or("", |m| m.as_str())),
                    None => c[0].to_string(),
                }
            }).to_string();
            let next = MDLINK.replace_all(&next, |c: &regex::Captures| {
                if is_external(&c[1]) { return c[0].to_string(); }
                let old = old_of(&decode(&c[1]));
                match old.as_deref().and_then(|o| map.get(o)) {
                    Some(newp) => {
                        let rel = relative(dirname(&note_now), newp).split('/').map(|s| utf8_percent_encode(s, URI_COMPONENT).to_string()).collect::<Vec<_>>().join("/");
                        let heading = c.get(2).map_or("", |m| m.as_str());
                        c[0].replacen(&format!("{}{heading}", &c[1]), &format!("{rel}{heading}"), 1)
                    }
                    None => c[0].to_string(),
                }
            }).to_string();
            if next != content && std::fs::write(&full, &next).is_ok() { changed.push(note_now); }
        }
        self.build(root);
        changed.sort();
        changed
    }

    /// Plain-text mentions of a note's name in other notes that don't already link to it.
    pub fn unlinked_mentions(&self, root: &Path, rel: &str, limit: usize) -> Vec<Value> {
        let name = strip_md(basename(rel));
        if name.chars().count() < 3 { return vec![]; }
        let Ok(re) = mention_re(name) else { return vec![] };
        let back: HashSet<String> = self.backlinks_of(rel).into_iter().collect();
        let mut out = vec![];
        for p in self.files.keys() {
            if p == rel || back.contains(p) || p.split('/').any(|s| s == "_studio") { continue; }
            let content = std::fs::read_to_string(join_rel(root, p)).unwrap_or_default();
            let (_, body) = split_frontmatter(&content);
            for (i, line) in strip_code(body).split('\n').enumerate() {
                if out.len() >= limit { return out; }
                if re.is_match(&WIKI.replace_all(line, "")).unwrap_or(false) {
                    out.push(json!({ "path": p, "line": i, "text": line.trim().chars().take(200).collect::<String>() }));
                }
            }
        }
        out
    }

    /// Turn the first plain-text mention of `target` on a body line of `in_path` into a [[link]].
    pub fn link_mention(&mut self, root: &Path, in_path: &str, line_no: usize, target: &str) -> Result<(), String> {
        let name = strip_md(basename(target)).to_string();
        let full = join_rel(root, in_path);
        let content = std::fs::read_to_string(&full).map_err(|e| e.to_string())?;
        let offset = FRONTMATTER.find(&content).map(|m| m.as_str().matches('\n').count()).unwrap_or(0);
        let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
        let i = line_no + offset;
        let re = mention_re(&name).map_err(|e| e.to_string())?;
        let Some(line) = lines.get(i).cloned() else { return Err("Mention not found (the note may have changed)".into()) };
        let Some(m) = re.find(&line).ok().flatten() else { return Err("Mention not found (the note may have changed)".into()) };
        let short = if self.resolve(&name, in_path).as_deref() == Some(target) { name.clone() } else { strip_md(target).to_string() };
        let found = m.as_str();
        let link = if found == short { format!("[[{short}]]") } else { format!("[[{short}|{found}]]") };
        lines[i] = format!("{}{}{}", &line[..m.start()], link, &line[m.end()..]);
        std::fs::write(&full, lines.join("\n")).map_err(|e| e.to_string())?;
        self.build(root);
        Ok(())
    }

    /// Full-text search over notes (case-insensitive), or notes with a #tag.
    pub fn search(&self, root: &Path, q: &str, limit: usize) -> Vec<Value> {
        let q = q.trim();
        if q.is_empty() { return vec![]; }
        if let Some(tag) = q.strip_prefix('#') {
            let tag = tag.to_lowercase();
            return self.files.iter().filter(|(_, f)| f.tags.iter().any(|t| *t == tag || t.starts_with(&format!("{tag}/"))))
                .take(limit).map(|(p, _)| json!({ "path": p, "line": 0, "text": format!("#{tag}") })).collect();
        }
        let needle = q.to_lowercase();
        let mut out = vec![];
        for p in self.files.keys() {
            if p.split('/').any(|s| s == "_studio") { continue; }
            let content = std::fs::read_to_string(join_rel(root, p)).unwrap_or_default();
            if let Some((i, line)) = content.split('\n').enumerate().find(|(_, l)| l.to_lowercase().contains(&needle)) {
                out.push(json!({ "path": p, "line": i, "text": line.trim().chars().take(200).collect::<String>() }));
                if out.len() >= limit { break; }
            }
        }
        out
    }
}

fn mention_re(name: &str) -> Result<fancy_regex::Regex, fancy_regex::Error> {
    fancy_regex::Regex::new(&format!(r"(?i)(?<![\[\w]){}(?![\w\]])", fancy_regex::escape(name)))
}

/// Collapse `.` and `..` segments (like Node's path.join) in a '/'-separated relative path.
fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for seg in p.split('/') {
        match seg { "" | "." => {}, ".." => { out.pop(); }, s => out.push(s) }
    }
    out.join("/")
}

/// Relative path from directory `from_dir` to file `to` (both vault-relative, '/'-separated).
fn relative(from_dir: &str, to: &str) -> String {
    let a: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<&str> = vec![".."; a.len() - common];
    parts.extend(&b[common..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vault(files: &[(&str, &str)]) -> (tempfile::TempDir, LinkIndex) {
        let d = tempfile::tempdir().unwrap();
        for (p, c) in files { let f = d.path().join(p); std::fs::create_dir_all(f.parent().unwrap()).unwrap(); std::fs::write(f, c).unwrap(); }
        let mut idx = LinkIndex::default(); idx.build(d.path()); (d, idx)
    }
    #[test]
    fn parse_links_tags_frontmatter() {
        let f = parse("---\ntags: [Study, '#exam']\n---\n# Title\nSee [[Note A#Part|alias]] and ![[img.png]] and [x](sub/Other%20Note.md) and [w](https://x.com/a.md) #Topic/sub `[[ignored]]`");
        assert_eq!(f.links.iter().map(|l| (l.target.as_str(), l.embed)).collect::<Vec<_>>(), [("Note A", false), ("img.png", true), ("sub/Other Note.md", false)]);
        assert_eq!(f.links[0].heading, "Part"); assert_eq!(f.links[0].alias, "alias");
        assert_eq!(f.tags, ["topic/sub", "study", "exam"]);
    }
    #[test]
    fn resolution_and_backlinks() {
        let (_d, idx) = vault(&[("A/Note.md", "[[Other]] [[B/Note]] [[Missing]]"), ("B/Note.md", "x"), ("B/deep/Other.md", "y"), ("Other.md", "z")]);
        assert_eq!(idx.resolve("Other", "A/Note.md").as_deref(), Some("Other.md")); // shortest path wins
        assert_eq!(idx.resolve("B/Note", "A/Note.md").as_deref(), Some("B/Note.md"));
        let l = idx.links_of("A/Note.md");
        assert_eq!(l.links, ["Other.md", "B/Note.md"]); assert_eq!(l.unresolved, ["Missing"]);
        assert_eq!(idx.backlinks_of("B/Note.md"), ["A/Note.md"]);
    }
    #[test]
    fn rename_rewrites_links() {
        let (d, mut idx) = vault(&[("P/Club.md", "Book: [[Hail Mary#Rocky|the alien]] and [md](../B/Hail%20Mary.md)"), ("B/Hail Mary.md", "x")]);
        std::fs::rename(d.path().join("B/Hail Mary.md"), d.path().join("B/Project Hail Mary.md")).unwrap();
        let changed = idx.rewrite_links(d.path(), &[("B/Hail Mary.md".into(), "B/Project Hail Mary.md".into())]);
        assert_eq!(changed, ["P/Club.md"]);
        let c = std::fs::read_to_string(d.path().join("P/Club.md")).unwrap();
        assert_eq!(c, "Book: [[Project Hail Mary#Rocky|the alien]] and [md](../B/Project%20Hail%20Mary.md)");
    }
    #[test]
    fn mentions_and_linking() {
        let (d, mut idx) = vault(&[("Goals.md", "---\na: 1\n---\nFinish the garden shed soon.\nAlready [[Garden Shed]] here"), ("Garden Shed.md", "x")]);
        // Goals already links once, so it's a backlink and not reported; use a separate note
        std::fs::write(d.path().join("Plan.md"), "Build the garden shed.").unwrap(); idx.build(d.path());
        let m = idx.unlinked_mentions(d.path(), "Garden Shed.md", 10);
        assert_eq!(m.len(), 1); assert_eq!(m[0]["path"], "Plan.md");
        idx.link_mention(d.path(), "Plan.md", 0, "Garden Shed.md").unwrap();
        assert_eq!(std::fs::read_to_string(d.path().join("Plan.md")).unwrap(), "Build the [[Garden Shed|garden shed]].");
    }
    #[test]
    fn relative_paths() { assert_eq!(relative("P/sub", "B/x.md"), "../../B/x.md"); assert_eq!(relative("", "B/x.md"), "B/x.md"); }
}
