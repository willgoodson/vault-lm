//! Retrieval: hybrid (vector + BM25, reciprocal rank fusion), always rooted in the notebook.
//!  reference (default) – the selection first, then the most relevant material from the rest of the notebook
//!  focused             – only the selection, plus the notebook overview as background context
use crate::error::Result;
use crate::index::{esc, fts_query, under_sql, Row, SearchIndex};
use crate::layout::{self, Notebook, Scope};
use crate::models::Models;
use crate::util::{join_rel, under};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

const K_PRIMARY: usize = 8;

pub fn target_where(s: &Scope) -> String {
    if let Some(t) = &s.topic { return format!("(path = '{}' OR {})", esc(t), under_sql(&layout::topic_sources_dir(t))); }
    under_sql(s.section.as_deref().unwrap_or(&s.notebook))
}

/// Vector search + keyword search fused with RRF (k=60). Rows keep `distance` and `fts_rank`.
pub async fn hybrid(idx: &SearchIndex, qv: &[f32], kw: &str, filter: Option<&str>, k: usize) -> Result<Vec<Row>> {
    let vec = idx.vector_search(qv, filter, k * 2).await?;
    let fts = if kw.is_empty() { vec![] } else { idx.text_search(kw, filter, k * 2).await };
    let key = |r: &Row| format!("{}\u{0}{}\u{0}{}", r.path, r.heading, r.text.chars().take(80).collect::<String>());
    let mut fused: Vec<(Row, f64)> = vec![];
    let mut pos: HashMap<String, usize> = HashMap::new();
    for (i, r) in vec.into_iter().enumerate() { pos.insert(key(&r), fused.len()); fused.push((r, 1.0 / (60.0 + i as f64))); }
    for (i, mut r) in fts.into_iter().enumerate() {
        let kk = key(&r);
        match pos.get(&kk) {
            Some(&j) => { fused[j].1 += 1.0 / (60.0 + i as f64); fused[j].0.fts_rank = Some(i); }
            None => { r.fts_rank = Some(i); if r.distance.is_none() { r.distance = Some(1.0); } pos.insert(kk, fused.len()); fused.push((r, 1.0 / (60.0 + i as f64))); }
        }
    }
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Ok(fused.into_iter().take(k).map(|(r, _)| r).collect())
}

fn origin_of(nb: Option<&Notebook>, p: &str) -> String {
    nb.and_then(|n| n.sections.iter().find(|s| p.starts_with(&format!("{}/", s.path))).map(|s| s.name.clone()))
        .or_else(|| nb.map(|n| n.name.clone())).unwrap_or_else(|| "notebook".into())
}

fn tag(r: Row, kind: &str, nb: Option<&Notebook>) -> Value {
    let origin = origin_of(nb, &r.path);
    let mut v = serde_json::to_value(&r).unwrap();
    v["kind"] = json!(kind); v["reference"] = json!(kind != "primary"); v["origin"] = json!(origin);
    v
}

/// Keep the top hits, anything semantically close, and any exact keyword match.
fn relevant(rows: Vec<Row>, cut: f32) -> Vec<Row> {
    rows.into_iter().enumerate().filter(|(i, r)| *i < 2 || r.distance.unwrap_or(1.0) <= cut || r.fts_rank.is_some_and(|f| f < 3)).map(|(_, r)| r).collect()
}

pub async fn retrieve(idx: &SearchIndex, models: &Models, root: &Path, data: &Path, question: &str, scope: &Scope) -> Result<Vec<Value>> {
    if idx.table.is_none() || scope.notebook.is_empty() { return Ok(vec![]); }
    let focused = scope.mode.as_deref() == Some("focused");
    let qv = models.embed(&[question.to_string()], "search_query").await?.remove(0);
    let kw = fts_query(question);
    let nbs = layout::list_notebooks(root, data);
    let nb = nbs.iter().find(|n| n.path == scope.notebook);
    let nb_where = under_sql(&scope.notebook);

    if scope.section.is_none() && scope.topic.is_none() {
        let rows = relevant(hybrid(idx, &qv, &kw, Some(&nb_where), K_PRIMARY + 2).await?, 0.6);
        return Ok(rows.into_iter().map(|r| tag(r, "primary", nb)).collect());
    }
    let target = target_where(scope);
    let primary: Vec<Value> = relevant(hybrid(idx, &qv, &kw, Some(&target), K_PRIMARY).await?, 0.6).into_iter().map(|r| tag(r, "primary", nb)).collect();
    if focused {
        let mut out = primary;
        if let Some(ov) = overview_text(root, &scope.notebook) {
            out.push(json!({ "note": "Notebook Overview", "heading": "", "path": overview_path(&scope.notebook), "text": ov, "kind": "overview", "reference": true, "origin": "overview" }));
        }
        return Ok(out);
    }
    let rest = hybrid(idx, &qv, &kw, Some(&format!("{nb_where} AND NOT {target}")), 8).await?;
    let close: Vec<Value> = rest.into_iter().filter(|r| r.distance.unwrap_or(1.0) <= 0.55 || r.fts_rank.is_some_and(|f| f < 3)).take(6).map(|r| tag(r, "notebook", nb)).collect();
    Ok(primary.into_iter().chain(close).collect())
}

/// All chunks of a scope in path order, for study tools (bounded by `max_chars`).
pub async fn scope_text(idx: &SearchIndex, scope: &Scope, max_chars: usize) -> Result<String> {
    let filter = if scope.section.is_some() || scope.topic.is_some() { target_where(scope) } else { under_sql(&scope.notebook) };
    let mut rows = idx.rows(Some(&filter), false, None).await?;
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    let mut out = String::new();
    for r in rows {
        let block = format!("\n### [[{}{}]]\n{}\n", r.note, if r.heading.is_empty() { String::new() } else { format!("#{}", r.heading) }, r.text);
        if out.len() + block.len() > max_chars { break; }
        out += &block;
    }
    Ok(out)
}

/// Average chunk embedding per note → nearest other notes (for "Related topics" and the graph).
pub async fn related(idx: &SearchIndex, note_path: &str, prefix: Option<&str>, limit: usize) -> Result<Vec<Value>> {
    let own = idx.rows(Some(&format!("path = '{}'", esc(note_path))), true, None).await?;
    let vecs: Vec<&Vec<f32>> = own.iter().filter_map(|r| r.vector.as_ref()).filter(|v| !v.is_empty()).collect();
    if vecs.is_empty() { return Ok(vec![]); }
    let dim = vecs[0].len();
    let mut avg = vec![0f32; dim];
    for v in &vecs { for (a, x) in avg.iter_mut().zip(v.iter()) { *a += x / vecs.len() as f32; } }
    let mut filter = format!("path != '{}' AND NOT {}", esc(note_path), under_sql(&layout::topic_sources_dir(note_path)));
    if let Some(p) = prefix.filter(|p| !p.is_empty()) { filter += &format!(" AND {}", under_sql(p)); }
    let mut best: HashMap<String, Row> = HashMap::new();
    for r in idx.vector_search(&avg, Some(&filter), limit * 6).await? {
        if r.path.contains("/_studio/") { continue; }
        let keep = best.get(&r.path).map_or(true, |b| b.distance > r.distance);
        if keep { best.insert(r.path.clone(), r); }
    }
    let mut v: Vec<Row> = best.into_values().collect();
    v.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
    Ok(v.into_iter().take(limit).map(|r| json!({ "path": r.path, "note": r.note, "heading": r.heading, "score": ((1.0 - r.distance.unwrap_or(1.0)) * 1000.0).round() / 1000.0 })).collect())
}

pub fn overview_path(nb: &str) -> String { format!("{nb}/_studio/Notebook Overview.md") }
pub fn overview_text(root: &Path, nb: &str) -> Option<String> {
    let raw = std::fs::read_to_string(join_rel(root, &overview_path(nb))).ok()?;
    let (_, body) = crate::links::split_frontmatter(&raw);
    let t: String = body.trim().chars().take(2500).collect();
    (!t.is_empty()).then_some(t)
}
pub fn in_notebook(p: &str, nb: &str) -> bool { under(p, nb) }
