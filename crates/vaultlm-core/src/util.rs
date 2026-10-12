//! Small helpers shared across modules. Vault-relative paths are always '/'-separated strings.
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Vault-relative path of `full` under `root`, with '/' separators.
pub fn rel_of(root: &Path, full: &Path) -> String {
    full.strip_prefix(root).unwrap_or(full).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Join a '/'-separated relative path onto `root`.
pub fn join_rel(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty() && *s != ".") { p.push(part); }
    p
}

pub fn dirname(rel: &str) -> &str { rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("") }
pub fn basename(rel: &str) -> &str { rel.rsplit_once('/').map(|(_, b)| b).unwrap_or(rel) }
pub fn stem(name: &str) -> &str { name.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map(|(s, _)| s).unwrap_or(name) }
pub fn ext(name: &str) -> &str { name.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map(|(_, e)| e).unwrap_or("") }
pub fn join(dir: &str, name: &str) -> String { if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") } }
pub fn strip_md(s: &str) -> &str { s.strip_suffix(".md").unwrap_or(s) }
pub fn under(p: &str, folder: &str) -> bool { p == folder || p.starts_with(&format!("{folder}/")) }

/// Strip characters that are invalid in file names or meaningful in wikilinks.
pub fn clean_name(n: &str) -> String {
    let s: String = n.chars().filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']')).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Same folder-name slug as the Node version, so existing per-vault data folders are reused.
pub fn slug(p: &str) -> String {
    let trimmed = p.trim_start_matches('/');
    let mut out = String::new();
    let mut in_sep = false;
    for c in trimmed.chars() {
        if c.is_ascii_alphanumeric() { out.push(c); in_sep = false; } else if !in_sep { out.push('-'); in_sep = true; }
    }
    let chars: Vec<char> = out.chars().collect();
    chars[chars.len().saturating_sub(80)..].iter().collect()
}

pub fn mtime_ms(p: &Path) -> Option<f64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_secs_f64() * 1000.0)
}

pub fn now_iso() -> String { chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string() }

/// Natural-ish sort key like `localeCompare(..., { numeric: true })`.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new(); while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) { na.push(c); ai.next(); }
                let mut nb = String::new(); while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) { nb.push(c); bi.next(); }
                let o = na.trim_start_matches('0').len().cmp(&nb.trim_start_matches('0').len()).then(na.cmp(&nb));
                if o != std::cmp::Ordering::Equal { return o; }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase()).then(x.cmp(&y));
                if o != std::cmp::Ordering::Equal { return o; }
                ai.next(); bi.next();
            }
        }
    }
}

/// Write via a unique temp file + rename so readers never see a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let tmp = path.with_extension(format!("{}.tmp", rand::random::<u64>()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

pub fn read_json(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slug_matches_node() {
        assert_eq!(slug("/Users/alice/Documents/Notebooks"), "Users-alice-Documents-Notebooks");
    }
    #[test]
    fn clean() { assert_eq!(clean_name("  a/b:c  [[x]]  "), "abc x"); }
    #[test]
    fn natural() { assert_eq!(natural_cmp("Task 2", "Task 10"), std::cmp::Ordering::Less); }
}
