//! Small per-vault JSON stores in the app data folder: details (meta), UI state, templates.
use crate::util::{self, write_atomic};
use serde_json::{json, Map, Value};
use std::path::Path;

// ---------- meta: notebook/section details, keyed by vault-relative folder path
const META_FIELDS: [&str; 5] = ["description", "goals", "status", "due", "color"];

pub fn load_meta(data: &Path) -> Map<String, Value> { util::read_json(&data.join("meta.json")).as_object().cloned().unwrap_or_default() }
fn save_meta(data: &Path, m: &Map<String, Value>) -> std::io::Result<()> { write_atomic(&data.join("meta.json"), serde_json::to_string_pretty(m)?.as_bytes()) }

pub fn set_meta(data: &Path, path: &str, patch: &Value) -> std::io::Result<Value> {
    let mut m = load_meta(data);
    let mut cur = m.get(path).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    for k in META_FIELDS {
        let Some(v) = patch.get(k) else { continue };
        let v = if k == "goals" {
            let list: Vec<Value> = match v { Value::Array(a) => a.clone(), other => vec![other.clone()] }
                .into_iter().filter_map(|g| g.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).map(Value::String).collect();
            Value::Array(list)
        } else { Value::String(v.as_str().map(|s| s.trim().to_string()).unwrap_or_default()) };
        let empty = matches!(&v, Value::Array(a) if a.is_empty()) || matches!(&v, Value::String(s) if s.is_empty());
        if empty { cur.remove(k); } else { cur.insert(k.to_string(), v); }
    }
    if cur.is_empty() { m.remove(path); } else { m.insert(path.to_string(), Value::Object(cur.clone())); }
    save_meta(data, &m)?;
    Ok(Value::Object(cur))
}

/// Re-key everything at or under `from` to `to` (renames/moves).
pub fn rekey_meta(data: &Path, from: &str, to: &str) -> std::io::Result<()> {
    let mut m = load_meta(data);
    let keys: Vec<String> = m.keys().filter(|k| util::under(k, from)).cloned().collect();
    if keys.is_empty() { return Ok(()); }
    for k in keys { if let Some(v) = m.remove(&k) { m.insert(format!("{to}{}", &k[from.len()..]), v); } }
    save_meta(data, &m)
}
pub fn drop_meta(data: &Path, path: &str) -> std::io::Result<()> {
    let mut m = load_meta(data);
    m.retain(|k, _| !util::under(k, path));
    save_meta(data, &m)
}

/// Prompt context describing what the user wants from the notebook/section.
pub fn goals_context(data: &Path, notebook: &str, section: Option<&str>) -> String {
    let m = load_meta(data);
    let mut parts = vec![];
    for (label, p) in [("Notebook", Some(notebook)), ("Section", section)] {
        let Some(p) = p.filter(|p| !p.is_empty()) else { continue };
        let Some(d) = m.get(p) else { continue };
        let desc = d["description"].as_str().unwrap_or("");
        let goals: Vec<&str> = d["goals"].as_array().into_iter().flatten().filter_map(|g| g.as_str()).collect();
        if desc.is_empty() && goals.is_empty() { continue; }
        let name = util::basename(p);
        let mut s = format!("{label} \"{name}\"");
        if !desc.is_empty() { s += &format!(": {desc}"); }
        if !goals.is_empty() { s += &format!("\nGoals:\n{}", goals.iter().map(|g| format!("- {g}")).collect::<Vec<_>>().join("\n")); }
        parts.push(s);
    }
    parts.join("\n\n")
}

// ---------- UI state (per-notebook tabs, selection, chats)
pub fn load_ui_state(data: &Path) -> Value { util::read_json(&data.join("ui-state.json")) }
pub fn save_ui_state(data: &Path, state: &Value) -> std::io::Result<()> {
    write_atomic(&data.join("ui-state.json"), serde_json::to_string(state)?.as_bytes())
}

// ---------- templates
pub fn default_templates() -> Value {
    json!([
      { "id": "lecture", "name": "Lecture note", "body": "# {{title}}\n\n**Date:** {{date}} · **Course:** {{notebook}}\n\n## Key ideas\n- \n\n## Details\n\n## Questions I still have\n- \n\n## Summary (in my own words)\n" },
      { "id": "paper", "name": "Paper summary", "body": "---\ntype: paper\nauthors: \nyear: \n---\n\n# {{title}}\n\n## Question\n\n## Method\n\n## Findings\n- \n\n## Limitations\n\n## How it connects\n- [[]]\n" },
      { "id": "chapter", "name": "Book chapter", "body": "# {{title}}\n\n> Book: {{section}}\n\n## Summary\n\n## Key quotes\n> \n\n## Characters / concepts\n- \n\n## My thoughts\n" },
      { "id": "meeting", "name": "Meeting", "body": "# {{title}}\n\n**When:** {{date}} {{time}}\n**Attendees:** \n\n## Agenda\n- \n\n## Notes\n\n## Decisions\n- \n\n## Action items\n- [ ] \n" },
      { "id": "brief", "name": "Project brief", "body": "# {{title}}\n\n## Goal\n\n## Why it matters\n\n## Scope\n- In: \n- Out: \n\n## Milestones\n- [ ] \n\n## Risks\n- \n" }
    ])
}
pub fn load_templates(data: &Path) -> Value {
    std::fs::read_to_string(data.join("templates.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_templates)
}
pub fn save_templates(data: &Path, list: &Value) -> std::io::Result<Value> {
    let clean: Vec<Value> = list.as_array().into_iter().flatten().filter(|t| t["name"].as_str().is_some_and(|n| !n.is_empty())).map(|t| {
        let name: String = t["name"].as_str().unwrap().chars().take(60).collect();
        let id = t["id"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| {
            let re = regex::Regex::new("[^a-z0-9]+").unwrap(); re.replace_all(&name.to_lowercase(), "-").to_string()
        });
        json!({ "id": id.chars().take(40).collect::<String>(), "name": name, "body": t["body"].as_str().unwrap_or("") })
    }).collect();
    let v = Value::Array(clean);
    write_atomic(&data.join("templates.json"), serde_json::to_string_pretty(&v)?.as_bytes())?;
    Ok(v)
}
pub fn fill_template(body: &str, vars: &[(&str, &str)]) -> String {
    let re = regex::Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();
    re.replace_all(body, |c: &regex::Captures| vars.iter().find(|(k, _)| *k == &c[1]).map(|(_, v)| v.to_string()).unwrap_or_else(|| c[0].to_string())).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meta_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        set_meta(d.path(), "P/Shed", &json!({ "description": " Build it ", "goals": ["a", "", "b"], "color": "sky" })).unwrap();
        rekey_meta(d.path(), "P/Shed", "P/Barn").unwrap();
        let m = load_meta(d.path());
        assert_eq!(m["P/Barn"]["goals"], json!(["a", "b"]));
        assert_eq!(goals_context(d.path(), "P", Some("P/Barn")), "Section \"Barn\": Build it\nGoals:\n- a\n- b");
        set_meta(d.path(), "P/Barn", &json!({ "description": "", "goals": [], "color": "" })).unwrap();
        assert!(load_meta(d.path()).is_empty());
    }
    #[test]
    fn template_fill() { assert_eq!(fill_template("# {{title}} {{x}}", &[("title", "Hi")]), "# Hi {{x}}"); }
}
