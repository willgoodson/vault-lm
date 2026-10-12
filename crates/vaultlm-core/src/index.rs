//! Search index: notes are chunked by heading, embedded, and stored in LanceDB alongside a BM25
//! full-text index. Same on-disk layout and schema as the Node version, so existing indexes are reused.
//! One index per embedding model (vectors from different models aren't comparable).
use crate::error::{ApiError, Result};
use crate::layout::SKIP_DIRS;
use crate::links::split_frontmatter;
use crate::models::Models;
use crate::util::{basename, join_rel, mtime_ms, rel_of, strip_md};
use arrow_array::cast::AsArray;
use arrow_array::types::Float32Type;
use arrow_array::{Array, ArrayRef, FixedSizeListArray, Float32Array, Float64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::index::scalar::{FtsIndexBuilder, FullTextSearchQuery};
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::{Connection, DistanceType, Table};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_CHARS: usize = 3200; // ~800 tokens
pub const TABLE: &str = "chunks";

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Row {
    pub text: String, pub heading: String, pub note: String, pub path: String, pub notebook: String, pub mtime: f64,
    #[serde(rename = "_distance", skip_serializing_if = "Option::is_none")] pub distance: Option<f32>,
    #[serde(skip)] pub fts_rank: Option<usize>,
    #[serde(skip)] pub vector: Option<Vec<f32>>,
}

pub fn esc(s: &str) -> String { s.replace('\'', "''") }
pub fn under_sql(folder: &str) -> String { format!("starts_with(path, '{}/')", esc(folder)) }

pub fn index_dir(vault_data: &Path, embed_id: &str) -> PathBuf {
    if embed_id == "ollama:nomic-embed-text" { vault_data.join("lancedb") }
    else { vault_data.join(format!("lancedb-{}", embed_id.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect::<String>())) }
}

/// Split a note into heading-scoped chunks (heading-only blocks are skipped).
pub fn chunk_note(content: &str) -> Vec<(String, String)> {
    let (_, body) = split_frontmatter(content);
    let heading_re = regex::Regex::new(r"^#{1,6}\s+(.*)").unwrap();
    let mut chunks = vec![];
    let (mut heading, mut buf): (String, Vec<&str>) = (String::new(), vec![]);
    let mut flush = |heading: &str, buf: &mut Vec<&str>| {
        let block = buf.join("\n");
        buf.clear();
        let block = block.trim();
        if block.lines().all(|l| l.trim().is_empty() || heading_re.is_match(l)) { return; }
        let chars: Vec<char> = block.chars().collect();
        for piece in chars.chunks(MAX_CHARS) {
            let p: String = piece.iter().collect();
            let p = p.trim();
            if !p.is_empty() { chunks.push((heading.to_string(), p.to_string())); }
        }
    };
    for line in body.split('\n') {
        if let Some(c) = heading_re.captures(line) { flush(&heading, &mut buf); heading = c[1].trim().to_string(); }
        buf.push(line);
    }
    flush(&heading, &mut buf);
    chunks
}

fn list_notes(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) { continue; }
        let p = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { list_notes(root, &p, out); } else if name.ends_with(".md") { out.push(p); }
    }
}

fn lance_err(e: lancedb::Error) -> ApiError { ApiError::new(500, format!("Search index: {e}")) }

pub struct SearchIndex { pub dir: PathBuf, pub db: Connection, pub table: Option<Table> }

impl SearchIndex {
    pub async fn open(dir: &Path) -> Result<SearchIndex> {
        std::fs::create_dir_all(dir)?;
        let db = lancedb::connect(&dir.to_string_lossy()).execute().await.map_err(lance_err)?;
        let names = db.table_names().execute().await.map_err(lance_err)?;
        let table = if names.iter().any(|n| n == TABLE) { Some(db.open_table(TABLE).execute().await.map_err(lance_err)?) } else { None };
        Ok(SearchIndex { dir: dir.to_path_buf(), db, table })
    }

    pub async fn count(&self) -> usize { match &self.table { Some(t) => t.count_rows(None).await.unwrap_or(0), None => 0 } }

    /// Rows for a filter (no vectors unless asked).
    pub async fn rows(&self, filter: Option<&str>, with_vector: bool, limit: Option<usize>) -> Result<Vec<Row>> {
        let Some(t) = &self.table else { return Ok(vec![]) };
        let mut cols = vec!["text", "heading", "note", "path", "notebook", "mtime"];
        if with_vector { cols.push("vector"); }
        let mut q = t.query().select(Select::columns(&cols));
        if let Some(f) = filter { q = q.only_if(f); }
        if let Some(l) = limit { q = q.limit(l); } else { q = q.limit(1_000_000); }
        let batches: Vec<RecordBatch> = q.execute().await.map_err(lance_err)?.try_collect().await.map_err(lance_err)?;
        Ok(batches.iter().flat_map(rows_of).collect())
    }

    pub async fn vector_search(&self, v: &[f32], filter: Option<&str>, k: usize) -> Result<Vec<Row>> {
        let Some(t) = &self.table else { return Ok(vec![]) };
        let mut q = t.query().nearest_to(v).map_err(lance_err)?.distance_type(DistanceType::Cosine).limit(k);
        if let Some(f) = filter { q = q.only_if(f); }
        let batches: Vec<RecordBatch> = q.execute().await.map_err(lance_err)?.try_collect().await.map_err(lance_err)?;
        Ok(batches.iter().flat_map(rows_of).collect())
    }

    /// BM25 keyword search; empty if there's no FTS index yet.
    pub async fn text_search(&self, query: &str, filter: Option<&str>, k: usize) -> Vec<Row> {
        let Some(t) = &self.table else { return vec![] };
        if query.trim().is_empty() { return vec![]; }
        let mut q = t.query().full_text_search(FullTextSearchQuery::new(query.to_string())).limit(k);
        if let Some(f) = filter { q = q.only_if(f); }
        let Ok(stream) = q.execute().await else { return vec![] };
        let batches: Vec<RecordBatch> = stream.try_collect().await.unwrap_or_default();
        batches.iter().flat_map(rows_of).collect()
    }

    async fn ensure_fts(&self, rebuild: bool) {
        let Some(t) = &self.table else { return };
        let has = t.list_indices().await.map(|v| v.iter().any(|i| i.columns.iter().any(|c| c == "text") && format!("{:?}", i.index_type).to_lowercase().contains("fts"))).unwrap_or(false);
        if has && !rebuild { return; }
        if let Err(e) = t.create_index(&["text"], Index::FTS(FtsIndexBuilder::default())).replace(true).execute().await { eprintln!("FTS index failed: {e}"); }
    }

    /// Incremental index: re-embed only files whose mtime changed; drop deleted files.
    /// `progress(done, total)` is called per file. Returns (files, changed, chunks).
    pub async fn sync(&mut self, root: &Path, models: &Models, mut progress: impl FnMut(usize, usize), cancelled: impl Fn() -> bool) -> Result<(usize, usize, usize)> {
        let known: HashMap<String, f64> = self.rows(None, false, None).await?.into_iter().map(|r| (r.path, r.mtime)).collect();
        let mut files = vec![]; list_notes(root, root, &mut files);
        let total = files.len();
        let mut changed = 0;
        let mut seen = std::collections::HashSet::new();
        for (i, full) in files.iter().enumerate() {
            if cancelled() { break; }
            let rel = rel_of(root, full);
            seen.insert(rel.clone());
            progress(i + 1, total);
            let Some(mtime) = mtime_ms(full) else { continue };
            if known.get(&rel).is_some_and(|m| (m - mtime).abs() < 0.5) { continue; }
            let content = std::fs::read_to_string(full).unwrap_or_default();
            let note = strip_md(basename(&rel)).to_string();
            let chunks = chunk_note(&content);
            let rows = if chunks.is_empty() { vec![] } else {
                let inputs: Vec<String> = chunks.iter().map(|(h, t)| format!("{note} {h}\n{t}")).collect();
                match models.embed(&inputs, "search_document").await {
                    Ok(vecs) => chunks.into_iter().zip(vecs).map(|((heading, text), v)| Row {
                        text, heading, note: note.clone(), notebook: rel.split('/').next().filter(|_| rel.contains('/')).unwrap_or("(root)").to_string(),
                        path: rel.clone(), mtime, vector: Some(v), ..Default::default()
                    }).collect::<Vec<_>>(),
                    // Keep the old rows when embedding fails (e.g. Ollama not running) — never wipe the index.
                    Err(e) => { if i == 0 || changed == 0 { eprintln!("embed failed for {rel}: {}", e.message); } return Err(e); }
                }
            };
            if let Some(t) = &self.table { t.delete(&format!("path = '{}'", esc(&rel))).await.map_err(lance_err)?; }
            if !rows.is_empty() {
                let schema = match &self.table { Some(t) => t.schema().await.map_err(lance_err)?, None => default_schema(rows[0].vector.as_ref().unwrap().len()) };
                let batch = to_batch(&schema, &rows)?;
                match &self.table {
                    Some(t) => { t.add(batch).execute().await.map_err(lance_err)?; }
                    None => { self.table = Some(self.db.create_table(TABLE, batch).execute().await.map_err(lance_err)?); }
                }
            }
            changed += 1;
        }
        if !cancelled() {
            if let Some(t) = &self.table {
                for rel in known.keys().filter(|k| !seen.contains(*k)) { t.delete(&format!("path = '{}'", esc(rel))).await.map_err(lance_err)?; changed += 1; }
            }
        }
        self.ensure_fts(changed > 0).await;
        Ok((total, changed, self.count().await))
    }
}

fn default_schema(dim: usize) -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim as i32), true),
        Field::new("text", DataType::Utf8, true), Field::new("heading", DataType::Utf8, true), Field::new("note", DataType::Utf8, true),
        Field::new("path", DataType::Utf8, true), Field::new("notebook", DataType::Utf8, true), Field::new("mtime", DataType::Float64, true),
    ]))
}

/// Build a batch that matches the table's existing schema exactly (types and field order).
fn to_batch(schema: &SchemaRef, rows: &[Row]) -> Result<RecordBatch> {
    let mut cols: Vec<ArrayRef> = vec![];
    for f in schema.fields() {
        let col: ArrayRef = match (f.name().as_str(), f.data_type()) {
            ("vector", DataType::FixedSizeList(inner, dim)) => {
                let vals: Vec<f32> = rows.iter().flat_map(|r| r.vector.clone().unwrap_or_default()).collect();
                let values: ArrayRef = match inner.data_type() {
                    DataType::Float64 => Arc::new(Float64Array::from(vals.iter().map(|x| *x as f64).collect::<Vec<_>>())),
                    _ => Arc::new(Float32Array::from(vals)),
                };
                Arc::new(FixedSizeListArray::try_new(inner.clone(), *dim, values, None).map_err(|e| ApiError::new(500, e.to_string()))?)
            }
            ("mtime", DataType::Float32) => Arc::new(Float32Array::from(rows.iter().map(|r| r.mtime as f32).collect::<Vec<_>>())),
            ("mtime", _) => Arc::new(Float64Array::from(rows.iter().map(|r| r.mtime).collect::<Vec<_>>())),
            (name, DataType::LargeUtf8) => Arc::new(arrow_array::LargeStringArray::from(rows.iter().map(|r| field(r, name)).collect::<Vec<_>>())),
            (name, _) => Arc::new(StringArray::from(rows.iter().map(|r| field(r, name)).collect::<Vec<_>>())),
        };
        cols.push(col);
    }
    RecordBatch::try_new(schema.clone(), cols).map_err(|e| ApiError::new(500, format!("Index schema mismatch: {e}")))
}
fn field<'a>(r: &'a Row, name: &str) -> &'a str {
    match name { "text" => &r.text, "heading" => &r.heading, "note" => &r.note, "path" => &r.path, "notebook" => &r.notebook, _ => "" }
}

fn str_col(b: &RecordBatch, name: &str, i: usize) -> String {
    let Some(c) = b.column_by_name(name) else { return String::new() };
    if c.is_null(i) { return String::new(); }
    if let Some(s) = c.as_string_opt::<i32>() { return s.value(i).to_string(); }
    if let Some(s) = c.as_string_opt::<i64>() { return s.value(i).to_string(); }
    String::new()
}
fn f64_col(b: &RecordBatch, name: &str, i: usize) -> Option<f64> {
    let c = b.column_by_name(name)?;
    if c.is_null(i) { return None; }
    c.as_primitive_opt::<arrow_array::types::Float64Type>().map(|a| a.value(i)).or_else(|| c.as_primitive_opt::<Float32Type>().map(|a| a.value(i) as f64))
}
fn rows_of(b: &RecordBatch) -> Vec<Row> {
    (0..b.num_rows()).map(|i| Row {
        text: str_col(b, "text", i), heading: str_col(b, "heading", i), note: str_col(b, "note", i), path: str_col(b, "path", i),
        notebook: str_col(b, "notebook", i), mtime: f64_col(b, "mtime", i).unwrap_or(0.0),
        distance: f64_col(b, "_distance", i).map(|d| d as f32),
        fts_rank: None,
        vector: b.column_by_name("vector").and_then(|c| c.as_fixed_size_list_opt().map(|l| {
            let v = l.value(i);
            v.as_primitive_opt::<Float32Type>().map(|a| a.values().to_vec())
                .or_else(|| v.as_primitive_opt::<arrow_array::types::Float64Type>().map(|a| a.values().iter().map(|x| *x as f32).collect()))
                .unwrap_or_default()
        })),
    }).collect()
}

/// Free text → safe keyword query (FTS syntax characters stripped).
pub fn fts_query(q: &str) -> String {
    q.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() || c == '\'' || c == '-' { c } else { ' ' }).collect::<String>()
        .split_whitespace().filter(|w| w.chars().count() > 2).take(24).collect::<Vec<_>>().join(" ")
}

pub fn root_of(p: &Path) -> &Path { p }
pub fn note_path(root: &Path, rel: &str) -> PathBuf { join_rel(root, rel) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunking() {
        let c = chunk_note("---\na: 1\n---\n# Title\n## Plot\nSome text.\n## Empty\n## Rocky\nMore.");
        assert_eq!(c.iter().map(|(h, _)| h.as_str()).collect::<Vec<_>>(), ["Plot", "Rocky"]);
        assert!(c[0].1.starts_with("## Plot\nSome text."));
        assert_eq!(fts_query("What's the xenonite, Rocky?"), "what's the xenonite rocky");
    }
}
