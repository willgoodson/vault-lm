//! Study progress: an append-only event log (progress.jsonl) of quiz answers, card grades and
//! explain-back scores, plus weak spots and dashboard stats. Same format as before.
use crate::layout::{list_notebooks, Notebook};
use crate::srs::{self, now_ms};
use crate::store::load_meta;
use crate::util::under;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const DAY: f64 = 86_400_000.0;
const HALF_LIFE: f64 = 14.0 * DAY;

pub fn log_event(data: &Path, mut e: Value) -> std::io::Result<()> {
    if e["kind"].as_str().unwrap_or("").is_empty() { return Ok(()); }
    e["t"] = json!(now_ms());
    std::fs::create_dir_all(data)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(data.join("progress.jsonl"))?;
    writeln!(f, "{e}")
}

pub fn events(data: &Path) -> Vec<Value> {
    std::fs::read_to_string(data.join("progress.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn parse_cite(c: &str) -> Option<(String, String)> {
    let re = regex::Regex::new(r"\[\[([^\]|#]+)(?:#([^\]|]+))?").unwrap();
    re.captures(c).map(|m| (m[1].trim().to_string(), m.get(2).map(|h| h.as_str().trim().to_string()).unwrap_or_default()))
}

/// Concepts you keep missing: recency-weighted accuracy per cited [[Note#Heading]].
pub fn weak_spots(data: &Path, notebook: Option<&str>, limit: usize) -> Vec<Value> {
    let now = now_ms();
    #[derive(Default)] struct S { note: String, heading: String, w: f64, right: f64, attempts: u32, misses: u32, last: f64, labels: Vec<String>, notebook: Value }
    let mut by: HashMap<String, S> = HashMap::new();
    for e in events(data) {
        if notebook.is_some_and(|nb| e["notebook"].as_str() != Some(nb)) { continue; }
        let concept = e["concept"].as_str().unwrap_or("").to_string();
        let Some((note, heading)) = parse_cite(&concept) else { continue };
        let t = e["t"].as_f64().unwrap_or(now);
        let w = 0.5f64.powf((now - t) / HALF_LIFE);
        let right = e["score"].as_f64().unwrap_or(if e["correct"].as_bool() == Some(true) { 1.0 } else { 0.0 });
        let s = by.entry(concept).or_insert_with(|| S { note, heading, notebook: e["notebook"].clone(), ..Default::default() });
        s.w += w; s.right += w * right; s.attempts += 1; if right < 0.5 { s.misses += 1; } s.last = s.last.max(t);
        if let Some(l) = e["label"].as_str() { if right < 0.5 && !s.labels.iter().any(|x| x == l) { s.labels.push(l.to_string()); } }
    }
    let mut v: Vec<(String, S, f64)> = by.into_iter().map(|(k, s)| { let acc = if s.w > 0.0 { s.right / s.w } else { 0.0 }; (k, s, acc) }).filter(|(_, s, acc)| s.misses > 0 && *acc < 0.8).collect();
    v.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap().then(b.1.last.partial_cmp(&a.1.last).unwrap()));
    v.into_iter().take(limit).map(|(concept, s, acc)| json!({
        "concept": concept, "note": s.note, "heading": s.heading, "attempts": s.attempts, "misses": s.misses, "last": s.last,
        "labels": s.labels.into_iter().take(3).collect::<Vec<_>>(), "accuracy": acc, "notebook": s.notebook,
    })).collect()
}

fn day_key(t: f64) -> String { chrono::DateTime::from_timestamp_millis(t as i64).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default() }

pub fn summary(root: &Path, data: &Path, notebook: Option<&str>) -> Value {
    let nbs: Vec<Notebook> = list_notebooks(root, data);
    let nb_of = |p: &str| nbs.iter().find(|n| under(p, &n.path)).map(|n| n.path.clone());
    let all = events(data);
    let evs: Vec<&Value> = all.iter().filter(|e| notebook.map_or(true, |nb| e["notebook"].as_str() == Some(nb))).collect();
    let db = srs::load(data);
    let now = now_ms();
    let cards: Vec<&Value> = db.values().filter(|c| notebook.map_or(true, |nb| nb_of(c["deck"].as_str().unwrap_or("")).as_deref() == Some(nb))).collect();
    let mut decks: Vec<Value> = vec![];
    let mut idx: HashMap<String, usize> = HashMap::new();
    for c in &cards {
        let nb = nb_of(c["deck"].as_str().unwrap_or("")).unwrap_or_else(|| "(other)".into());
        let i = *idx.entry(nb.clone()).or_insert_with(|| { decks.push(json!({ "notebook": nb, "name": nb.rsplit('/').next().unwrap_or(&nb), "total": 0, "due": 0, "learned": 0 })); decks.len() - 1 });
        let d = &mut decks[i];
        d["total"] = json!(d["total"].as_u64().unwrap() + 1);
        if c["due"].as_f64().unwrap_or(0.0) <= now { d["due"] = json!(d["due"].as_u64().unwrap() + 1); }
        if c["reps"].as_u64().unwrap_or(0) > 0 { d["learned"] = json!(d["learned"].as_u64().unwrap() + 1); }
    }
    decks.sort_by(|a, b| b["due"].as_u64().cmp(&a["due"].as_u64()));
    let mut days: HashMap<String, u32> = HashMap::new();
    let mut acc: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for e in &evs {
        let k = day_key(e["t"].as_f64().unwrap_or(now));
        *days.entry(k.clone()).or_default() += 1;
        if e["kind"] == "quiz" { let a = acc.entry(k).or_default(); a.1 += 1; if e["correct"].as_bool() == Some(true) { a.0 += 1; } }
    }
    let heat: Vec<Value> = (0..84).rev().map(|i| { let k = day_key(now - i as f64 * DAY); json!({ "date": k, "count": days.get(&k).copied().unwrap_or(0) }) }).collect();
    let streak = heat.iter().rev().take_while(|h| h["count"].as_u64().unwrap_or(0) > 0).count();
    let accuracy: Vec<Value> = acc.iter().rev().take(30).rev().map(|(d, (r, n))| json!({ "date": d, "pct": *r as f64 / *n as f64 })).collect();
    let meta = load_meta(data);
    let mut upcoming: Vec<Value> = meta.iter().filter(|(p, m)| m["due"].as_str().is_some() && notebook.map_or(true, |nb| under(p, nb)) && m["status"] != "done").map(|(p, m)| {
        let due = m["due"].as_str().unwrap();
        let due_at = chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(23, 59, 0)).and_then(|d| d.and_local_timezone(chrono::Local).single()).map(|d| d.timestamp_millis() as f64).unwrap_or(now);
        let in_scope: Vec<&Value> = db.values().filter(|c| under(c["deck"].as_str().unwrap_or(""), p)).collect();
        json!({ "path": p, "name": p.rsplit('/').next().unwrap_or(p), "due": due, "daysLeft": ((due_at - now) / DAY).ceil(),
            "cardsDueBefore": in_scope.iter().filter(|c| c["due"].as_f64().unwrap_or(0.0) <= due_at).count(), "total": in_scope.len() })
    }).collect();
    upcoming.sort_by(|a, b| a["due"].as_str().cmp(&b["due"].as_str()));
    json!({
        "decks": decks, "dueTotal": cards.iter().filter(|c| c["due"].as_f64().unwrap_or(0.0) <= now).count(),
        "learned": cards.iter().filter(|c| c["reps"].as_u64().unwrap_or(0) > 0).count(), "totalCards": cards.len(),
        "heat": heat, "streak": streak, "accuracy": accuracy, "quizAnswered": evs.iter().filter(|e| e["kind"] == "quiz").count(),
        "weak": weak_spots(data, notebook, 8), "upcoming": upcoming,
    })
}

pub fn due_cards(data: &Path, notebook: Option<&str>, limit: usize) -> Vec<Value> {
    let now = now_ms();
    let mut v: Vec<Value> = srs::load(data).into_values().filter(|c| c["due"].as_f64().unwrap_or(0.0) <= now && notebook.map_or(true, |nb| under(c["deck"].as_str().unwrap_or(""), nb))).collect();
    v.sort_by(|a, b| a["due"].as_f64().partial_cmp(&b["due"].as_f64()).unwrap());
    v.truncate(limit);
    v
}
