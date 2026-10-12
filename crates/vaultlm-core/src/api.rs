//! Route-style dispatcher: same paths and JSON shapes as the original HTTP API, so the Vue UI
//! keeps working through a single Tauri `api` command.
use crate::error::{ApiError, Result};
use crate::files::Files;
use crate::layout;
use crate::util::{self, basename, dirname, join, stem, under};
use crate::{store, vaults, Core};
use serde_json::{json, Value};
use std::path::PathBuf;

fn s<'a>(v: &'a Value, k: &str) -> &'a str { v.get(k).and_then(|x| x.as_str()).unwrap_or("") }
fn keyed(k: &str, v: Value) -> Value { let mut m = serde_json::Map::new(); m.insert(k.to_string(), v); Value::Object(m) }
fn scope_of(b: &Value) -> layout::Scope { serde_json::from_value(b.clone()).unwrap_or_default() }
fn home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix('~') {
        if let Some(h) = directories::BaseDirs::new() { return h.home_dir().join(rest.trim_start_matches('/')); }
    }
    PathBuf::from(p)
}

impl Core {
    pub async fn api(&self, method: &str, path: &str, query: Value, body: Value) -> Result<Value> {
        let m = method.to_ascii_uppercase();
        match (m.as_str(), path) {
            // ---------- app / vaults
            ("GET", "/api/info") => {
                let v = self.vault().ok();
                Ok(json!({
                    "vault": v.as_ref().map(|v| v.path.clone()), "vaultName": v.as_ref().map(|v| v.name.clone()),
                    "model": self.models.chat_label(), "provider": self.models.config().chat.provider, "cloud": self.models.is_cloud(), "embedModel": self.models.embed_id(),
                    "briefs": ["summary", "faq", "study-guide", "timeline"], "desktop": true, "version": env!("CARGO_PKG_VERSION"),
                }))
            }
            ("GET", "/api/vaults") => Ok(Value::Array(vaults::known(&self.data_root, self.vault().ok().as_ref()))),
            ("POST", "/api/vaults/select") => {
                let v = self.open_vault(&home(s(&body, "path")))?;
                Ok(json!({ "name": v.name, "path": v.path }))
            }
            ("POST", "/api/vaults/create") => {
                let p = home(s(&body, "path"));
                if p.exists() { return Err(ApiError::conflict(format!("{} already exists. Use “Open folder” instead.", p.display()))); }
                std::fs::create_dir_all(&p)?;
                let v = self.open_vault(&p)?;
                Ok(json!({ "name": v.name, "path": v.path }))
            }
            ("GET", "/api/index/status") => { let mut st = self.status.read().clone(); st["overview"] = json!(*self.overview_running.read()); Ok(st) }
            ("POST", "/api/index") => { self.reindex(); Ok(self.status.read().clone()) }

            // ---------- models (app-wide)
            ("GET", "/api/models") => Ok(self.models.public_config()),
            ("PUT", "/api/models") => {
                let before = self.models.embed_id();
                let mut cfg = self.models.update(&body)?;
                let reindexed = self.models.embed_id() != before;
                if reindexed { *self.reader.lock() = None; self.reindex(); }
                cfg["reindexed"] = json!(reindexed);
                self.emit("models", json!({ "chat": self.models.chat_label(), "cloud": self.models.is_cloud() }));
                Ok(cfg)
            }
            ("GET", "/api/models/list") => self.models.list(match s(&query, "provider") { "" => "ollama", p => p }).await.map(Value::Array).map_err(|e| ApiError::bad_request(e.message)),
            ("POST", "/api/models/test") => {
                // Try the submitted settings; keep them only if the provider answers.
                let provider = s(&body, "provider").to_string();
                let before = self.models.config();
                if let Some(settings) = body.get("settings") { self.models.update(&keyed(&provider, settings.clone()))?; }
                let t = std::time::Instant::now();
                match self.models.list(&provider).await {
                    Ok(list) => Ok(json!({ "ok": true, "models": list.len(), "ms": t.elapsed().as_millis() as u64 })),
                    Err(e) => {
                        let mut restore = serde_json::to_value(&before)?;
                        if let Some(o) = restore.get_mut(&provider).and_then(|x| x.as_object_mut()) { o.remove("apiKey"); }
                        let _ = self.models.update(&keyed(&provider, restore[&provider].clone()));
                        if body.pointer("/settings/apiKey").and_then(|k| k.as_str()).is_some_and(|k| !k.is_empty()) { let _ = self.models.update(&keyed(&provider, json!({ "apiKey": "" }))); }
                        Err(ApiError::bad_request(e.message))
                    }
                }
            }
            ("POST", "/api/models/pull") => {
                let name = s(&body, "name").trim().to_string();
                if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || "._-/:".contains(c)) { return Err(ApiError::bad_request("Invalid model name")); }
                let (Some(core), Some(rt)) = (self.me.upgrade(), self.rt.clone()) else { return Err(ApiError::new(500, "No runtime")) };
                let id = format!("pull-{}", crate::srs::now_ms() as u64);
                rt.spawn(async move {
                    let label = format!("Downloading {name}");
                    let res = core.models.pull(&name, |status, done, total| core.emit("job", json!({ "id": id, "label": label, "done": done, "total": total.max(1), "note": status }))).await;
                    core.emit("job", json!({ "id": id, "label": name, "finished": true, "error": res.err().map(|e| e.message) }));
                });
                Ok(json!({ "started": true }))
            }
            ("GET", "/api/ui-state") => Ok(store::load_ui_state(&self.vault()?.data)),
            ("PUT", "/api/ui-state") => {
                let v = self.vault()?;
                let _guard = self.ui_lock.lock(); // serialize writes
                store::save_ui_state(&v.data, &body)?;
                Ok(json!({ "ok": true }))
            }

            // ---------- layout
            ("GET", "/api/notebooks") => {
                let v = self.vault()?;
                let meta = store::load_meta(&v.data);
                let mut nbs = layout::list_notebooks(&v.path, &v.data);
                for nb in &mut nbs {
                    nb.meta = Some(meta.get(&nb.path).cloned().unwrap_or_else(|| json!({})));
                    for sec in &mut nb.sections { sec.meta = Some(meta.get(&sec.path).cloned().unwrap_or_else(|| json!({}))); }
                }
                Ok(serde_json::to_value(nbs)?)
            }
            ("GET", "/api/meta") => Ok(Value::Object(store::load_meta(&self.vault()?.data))),
            ("PUT", "/api/meta") => {
                let r = store::set_meta(&self.vault()?.data, s(&body, "path"), body.get("patch").unwrap_or(&json!({})))?;
                self.emit("tree", json!({}));
                Ok(r)
            }
            ("GET", "/api/layout") => {
                let v = self.vault()?;
                Ok(json!({ "root": layout::get_settings(&v.data).root, "folders": layout::top_folders(&v.path) }))
            }
            ("PUT", "/api/layout") => {
                let r = layout::save_settings(&self.vault()?.data, s(&body, "root"))?;
                self.emit("tree", json!({}));
                Ok(serde_json::to_value(r)?)
            }
            ("GET", "/api/tree") => Ok(tree(&self.vault()?.path, &self.vault()?.path)),
            ("POST", "/api/folder") => {
                let v = self.vault()?;
                let parent = body.get("parent").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(String::from).unwrap_or_else(|| layout::get_settings(&v.data).root);
                let rel = Files::new(&v.path).create_folder(&parent, s(&body, "name"))?;
                self.emit("tree", json!({}));
                Ok(json!({ "path": rel }))
            }
            ("GET", "/api/templates") => Ok(store::load_templates(&self.vault()?.data)),
            ("PUT", "/api/templates") => Ok(store::save_templates(&self.vault()?.data, &body)?),
            ("POST", "/api/templates/reset") => Ok(store::save_templates(&self.vault()?.data, &store::default_templates())?),
            ("POST", "/api/topic") => {
                let v = self.vault()?;
                let files = Files::new(&v.path);
                let folder = s(&body, "folder");
                let tpl_id = s(&body, "template");
                let tpl = (!tpl_id.is_empty()).then(|| store::load_templates(&v.data)).and_then(|t| t.as_array().and_then(|a| a.iter().find(|x| x["id"] == tpl_id).cloned()));
                let rel = if let Some(tpl) = tpl {
                    let name = util::clean_name(s(&body, "name"));
                    let nbs = layout::list_notebooks(&v.path, &v.data);
                    let nb = nbs.iter().find(|n| under(folder, &n.path));
                    let sec = nb.and_then(|n| n.sections.iter().find(|x| under(folder, &x.path)));
                    let now = chrono::Local::now();
                    let (date, time) = (now.format("%Y-%m-%d").to_string(), now.format("%H:%M").to_string());
                    let nb_name = nb.map(|n| n.name.clone()).unwrap_or_default();
                    let sec_name = sec.map(|x| x.name.clone()).unwrap_or_else(|| nb_name.clone());
                    let content = store::fill_template(tpl["body"].as_str().unwrap_or(""), &[("title", &name), ("date", &date), ("time", &time), ("notebook", &nb_name), ("section", &sec_name)]);
                    files.create_note(folder, &name, Some(&content))?
                } else { files.create_note(folder, s(&body, "name"), None)? };
                self.links.lock().build(&v.path);
                self.emit("tree", json!({}));
                Ok(json!({ "path": rel }))
            }
            ("POST", "/api/rename") => {
                let v = self.vault()?;
                let from = s(&body, "path").to_string();
                let is_dir = Files::new(&v.path).safe(&from)?.is_dir();
                let name = util::clean_name(s(&body, "name"));
                if name.is_empty() { return Err(ApiError::bad_request("Name required")); }
                let file_ext = util::ext(basename(&from));
                let to = join(dirname(&from), &if is_dir { name } else { format!("{name}.{}", if file_ext.is_empty() { "md" } else { file_ext }) });
                self.relocate(&from, &to)
            }
            ("POST", "/api/move") => {
                let from = s(&body, "path").to_string();
                let to_folder = s(&body, "toFolder").trim_end_matches('/');
                if dirname(&from) == to_folder { return Ok(json!({ "path": from, "moved": [], "updated": [] })); }
                self.relocate(&from, &join(to_folder, basename(&from)))
            }
            ("POST", "/api/trash") => {
                let v = self.vault()?;
                let rel = s(&body, "path");
                let to = Files::new(&v.path).trash(rel)?;
                store::drop_meta(&v.data, rel)?;
                self.links.lock().build(&v.path);
                self.emit("tree", json!({}));
                Ok(json!({ "trashed": to }))
            }
            ("POST", "/api/audio") => self.start_audio(scope_of(&body)),
            ("POST", "/api/audio/status") => {
                let sc = scope_of(&body);
                Ok(self.audio_status(&format!("{}|{}", self.vault()?.path.display(), sc.topic.clone().unwrap_or_else(|| sc.base()))))
            }
            ("POST", "/api/source/url") => {
                let dir = scope_of(&body).sources_dir();
                Ok(json!({ "path": self.add_url(&dir, s(&body, "url").trim()).await? }))
            }
            ("POST", "/api/source/file") | ("POST", "/api/source/pdf") => {
                // Scope comes in the query (as before); files arrive base64-encoded over IPC.
                let dir = scope_of(&query).sources_dir();
                let (mut saved, mut failed) = (vec![], vec![]);
                for f in body["files"].as_array().into_iter().flatten() {
                    let name = s(f, "name").to_string();
                    let res = match decode_b64(s(f, "data")) { Ok(bytes) => self.add_file(&dir, &name, &bytes, None, None).await, Err(e) => Err(e) };
                    match res { Ok(p) => saved.push(json!(p)), Err(e) => failed.push(json!({ "name": name, "error": e.message })) }
                }
                if saved.is_empty() && !failed.is_empty() {
                    return Err(ApiError::bad_request(failed.iter().map(|f| format!("{}: {}", f["name"].as_str().unwrap_or(""), f["error"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("; ")));
                }
                self.emit("tree", json!({}));
                self.reindex();
                Ok(json!({ "paths": saved, "failed": failed }))
            }
            ("POST", "/api/source/reprocess") => { let p = self.reprocess(s(&body, "path")).await?; self.emit("tree", json!({})); self.reindex(); Ok(json!({ "path": p })) }
            ("POST", "/api/source/remove") => {
                let v = self.vault()?;
                let files = Files::new(&v.path);
                let rel = s(&body, "path");
                if !rel.split('/').any(|p| p == "sources") { return Err(ApiError::bad_request("Not a source")); }
                let (content, _) = files.read(rel)?;
                let (fm, _) = crate::links::split_frontmatter(&content);
                let mut trashed = vec![files.trash(rel)?];
                if let Some(orig) = fm["original"].as_str().filter(|o| o.split('/').any(|p| p == "sources") && files.exists(o)) {
                    trashed.push(files.trash(orig)?);
                    files.remove_if_empty(dirname(orig));
                }
                files.remove_if_empty(dirname(rel));
                self.links.lock().build(&v.path);
                self.emit("tree", json!({}));
                Ok(json!({ "trashed": trashed }))
            }

            // ---------- notes
            ("GET", "/api/palette") => {
                let idx = self.links.lock();
                let mut out = vec![];
                for (p, f) in idx.files.iter() {
                    let note = stem(basename(p));
                    out.push(json!({ "path": p, "note": note, "heading": "", "tags": f.tags }));
                    for (_, h) in &f.headings { if h != note { out.push(json!({ "path": p, "note": note, "heading": h })); } }
                }
                Ok(Value::Array(out))
            }
            ("GET", "/api/tags") => Ok(Value::Array(self.links.lock().all_tags())),
            ("GET", "/api/note") => {
                let v = self.vault()?;
                let rel = s(&query, "path");
                let (content, mtime) = Files::new(&v.path).read(rel)?;
                let idx = self.links.lock();
                let parsed = crate::links::parse(&content);
                let visible = |ps: Vec<String>| ps.into_iter().filter(|p| !p.split('/').any(|seg| layout::SKIP_DIRS.contains(&seg))).collect::<Vec<_>>();
                let l = idx.links_of(rel);
                Ok(json!({
                    "path": rel, "content": content, "mtime": mtime,
                    "outline": parsed.headings.iter().map(|(level, heading)| json!({ "level": level, "heading": heading })).collect::<Vec<_>>(),
                    "links": visible(l.links), "unresolved": l.unresolved, "backlinks": visible(idx.backlinks_of(rel)),
                    "tags": idx.tags_of(rel), "properties": idx.frontmatter_of(rel),
                }))
            }
            ("PUT", "/api/note") => {
                let v = self.vault()?;
                let base = body.get("baseMtime").and_then(|m| m.as_f64());
                let mtime = Files::new(&v.path).save(s(&body, "path"), s(&body, "content"), base)?;
                Ok(json!({ "mtime": mtime }))
            }
            ("GET", "/api/note/mentions") => {
                let v = self.vault()?;
                Ok(Value::Array(self.links.lock().unlinked_mentions(&v.path, s(&query, "path"), 30)))
            }
            ("POST", "/api/note/link-mention") => {
                let v = self.vault()?;
                let line = body.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                self.links.lock().link_mention(&v.path, s(&body, "in"), line, s(&body, "target")).map_err(ApiError::conflict)?;
                Ok(json!({ "ok": true }))
            }
            ("GET", "/api/search") => {
                // ⌘K full text: BM25 index first, falling back to a file scan (and for #tags).
                let v = self.vault()?;
                let q = s(&query, "q").trim().to_string();
                if !q.is_empty() && !q.starts_with('#') {
                    if let Ok(idx) = self.reader().await {
                        let kw = crate::index::fts_query(&q);
                        let rows = idx.text_search(if kw.is_empty() { &q } else { &kw }, Some("path NOT LIKE '%/_studio/%'"), 40).await;
                        let needle = q.to_lowercase(); let first = needle.split_whitespace().next().unwrap_or("").to_string();
                        let mut seen = std::collections::HashSet::new();
                        let out: Vec<Value> = rows.into_iter().filter(|r| seen.insert(r.path.clone())).map(|r| {
                            let line = r.text.lines().find(|l| l.to_lowercase().contains(&first)).or_else(|| r.text.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#'))).unwrap_or("");
                            json!({ "path": r.path, "line": 0, "heading": r.heading, "text": line.trim().chars().take(200).collect::<String>() })
                        }).collect();
                        if !out.is_empty() { return Ok(Value::Array(out)); }
                    }
                }
                Ok(Value::Array(self.links.lock().search(&v.path, &q, 40)))
            }
            ("GET", "/api/graph") => {
                let nb = s(&query, "notebook").to_string();
                let mut g = self.links.lock().graph((!nb.is_empty()).then_some(nb.as_str()));
                let nodes: Vec<String> = g["nodes"].as_array().into_iter().flatten().filter_map(|n| n.as_str()).filter(|p| !p.split('/').any(|seg| layout::SKIP_DIRS.contains(&seg))).map(String::from).collect();
                let set: std::collections::HashSet<&str> = nodes.iter().map(|x| x.as_str()).collect();
                let edges: Vec<Value> = g["edges"].as_array().into_iter().flatten().filter(|e| set.contains(e["source"].as_str().unwrap_or("")) && set.contains(e["target"].as_str().unwrap_or(""))).cloned().collect();
                let mut similar = vec![];
                if !s(&query, "similar").is_empty() {
                    let idx = self.reader().await?;
                    let mut seen: std::collections::HashSet<String> = edges.iter().map(|e| { let mut k = [e["source"].as_str().unwrap_or(""), e["target"].as_str().unwrap_or("")]; k.sort(); k.join("|") }).collect();
                    for p in &nodes {
                        for r in crate::retrieve::related(&idx, p, Some(&nb), 2).await? {
                            let t = r["path"].as_str().unwrap_or("").to_string();
                            let mut k = [p.as_str(), t.as_str()]; k.sort();
                            if r["score"].as_f64().unwrap_or(0.0) >= 0.55 && set.contains(t.as_str()) && seen.insert(k.join("|")) { similar.push(json!({ "source": p, "target": t, "score": r["score"] })); }
                        }
                    }
                }
                g["nodes"] = json!(nodes); g["edges"] = json!(edges); g["similar"] = json!(similar);
                Ok(g)
            }
            ("GET", "/api/note/related") => {
                let idx = self.reader().await?;
                let nb = s(&query, "notebook");
                Ok(Value::Array(crate::retrieve::related(&idx, s(&query, "path"), (!nb.is_empty()).then_some(nb), 8).await?))
            }
            ("POST", "/api/note/suggest-links") => self.suggest_links(s(&body, "path"), s(&body, "notebook")).await,

            // ---------- chat helpers
            ("POST", "/api/chat/title") => Ok(json!({ "title": self.chat_title(s(&body, "question"), s(&body, "answer")).await? })),
            ("POST", "/api/topic/from-answer") => {
                let v = self.vault()?;
                let files = Files::new(&v.path);
                let title = util::clean_name(match s(&body, "title") { "" => "Saved answer", t => t });
                let mut cites: Vec<String> = vec![];
                for c in body["sources"].as_array().into_iter().flatten().filter(|c| c["kind"] != "overview") {
                    let h = c["heading"].as_str().unwrap_or("");
                    let t = format!("[[{}{}]]", c["note"].as_str().unwrap_or(""), if h.is_empty() { String::new() } else { format!("#{h}") });
                    if !cites.contains(&t) { cites.push(t); }
                }
                let block = format!("> **Q:** {}\n\n{}\n{}", s(&body, "question").replace('\n', "\n> "), s(&body, "answer").trim(), if cites.is_empty() { String::new() } else { format!("\n**Sources:** {}\n", cites.join(" · ")) });
                let rel = match body["appendTo"].as_str().filter(|p| !p.is_empty()) {
                    Some(target) => { let (content, _) = files.read(target)?; files.write(target, &format!("{}\n\n## {title}\n\n{block}", content.trim_end()))?; target.to_string() }
                    None => files.create_note(s(&body, "folder"), &title, Some(&format!("---\ncreated: {}\nfrom: chat\n---\n\n# {title}\n\n{block}", util::now_iso())))?,
                };
                self.links.lock().build(&v.path);
                self.emit("tree", json!({}));
                Ok(json!({ "path": rel }))
            }

            // ---------- studio
            ("POST", "/api/brief") => self.brief(&scope_of(&body), s(&body, "type")).await,
            ("POST", "/api/study/flashcards") => self.flashcards(&scope_of(&body), body["count"].as_u64().unwrap_or(15)).await,
            ("POST", "/api/study/deck") => self.deck(&scope_of(&body)),
            ("POST", "/api/study/review") => self.review(s(&body, "id"), body["grade"].as_u64().unwrap_or(2) as u8),
            ("POST", "/api/study/quiz") => self.quiz(&scope_of(&body), body["count"].as_u64().unwrap_or(5), match s(&body, "difficulty") { "" => "medium", d => d }, None).await,
            ("POST", "/api/study/mindmap") => self.mindmap(&scope_of(&body)).await,
            ("POST", "/api/study/explain") => self.explain_back(&scope_of(&body), s(&body, "subject"), s(&body, "explanation")).await,
            ("POST", "/api/study/card") => self.add_card(&scope_of(&body), &body["card"]),
            ("POST", "/api/study/drill") => {
                let concepts: Vec<String> = body["concepts"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
                self.drill(&scope_of(&body), &concepts, body["count"].as_u64().unwrap_or(5)).await
            }
            ("POST", "/api/assist") => self.assist(s(&body, "action"), s(&body, "text"), &scope_of(&body)).await,
            ("POST", "/api/progress") => { crate::progress::log_event(&self.vault()?.data, body.clone())?; Ok(json!({ "ok": true })) }
            ("GET", "/api/progress/summary") => { let v = self.vault()?; let nb = s(&query, "notebook"); Ok(crate::progress::summary(&v.path, &v.data, (!nb.is_empty()).then_some(nb))) }
            ("GET", "/api/progress/due") => { let nb = s(&query, "notebook"); Ok(Value::Array(crate::progress::due_cards(&self.vault()?.data, (!nb.is_empty()).then_some(nb), 50))) }
            ("GET", "/api/resolve") => Ok(json!({ "path": self.links.lock().resolve(s(&query, "note"), s(&query, "from")) })),
            ("POST", "/api/attachment") => {
                let v = self.vault()?;
                let file = body["files"].get(0).ok_or_else(|| ApiError::bad_request("No file"))?;
                let bytes = decode_b64(s(file, "data"))?;
                let (path, name) = Files::new(&v.path).save_attachment(s(&query, "note"), s(file, "name"), &bytes)?;
                self.links.lock().build(&v.path);
                Ok(json!({ "path": path, "name": name }))
            }

            // ---------- later phases
            _ if path.starts_with("/api/") => Err(ApiError::new(501, "This feature is coming in a later build of the desktop app.")),
            _ => Err(ApiError::not_found(format!("Unknown route {method} {path}"))),
        }
    }

    /// Streaming routes (chat): events are delivered through `on` as they're produced.
    pub async fn stream(&self, path: &str, body: Value, on: &(dyn Fn(Value) + Send + Sync)) -> Result<()> {
        match path {
            "/api/chat" => self.chat_stream(&body, on).await,
            _ => Err(ApiError::not_found(format!("Unknown stream {path}"))),
        }
    }

    /// Move a note or folder, rewriting [[links]] vault-wide and re-keying app data.
    /// A topic's sidecar sources folder moves with it.
    fn relocate(&self, from: &str, to: &str) -> Result<Value> {
        let v = self.vault()?;
        let files = Files::new(&v.path);
        let is_dir = files.safe(from)?.is_dir();
        if to == from { return Ok(json!({ "path": to, "moved": [], "updated": [] })); }
        if is_dir && format!("{to}/").starts_with(&format!("{from}/")) { return Err(ApiError::bad_request("Can't move a folder into itself")); }
        let mut idx = self.links.lock();
        idx.build(&v.path);
        let mut moved: Vec<(String, String)> = if is_dir {
            idx.all.iter().filter(|p| p.starts_with(&format!("{from}/"))).map(|p| (p.clone(), format!("{to}{}", &p[from.len()..]))).collect()
        } else { vec![(from.to_string(), to.to_string())] };
        let sidecar_from = (!is_dir).then(|| layout::topic_sources_dir(from));
        let has_sidecar = sidecar_from.as_deref().is_some_and(|s| files.exists(s));
        files.move_to(from, to)?;
        if let (true, Some(sf)) = (has_sidecar, sidecar_from) {
            let st = layout::topic_sources_dir(to);
            moved.extend(idx.all.iter().filter(|p| p.starts_with(&format!("{sf}/"))).map(|p| (p.clone(), format!("{st}{}", &p[sf.len()..]))));
            files.move_to(&sf, &st)?;
            files.remove_if_empty(dirname(&sf));
        }
        let updated = idx.rewrite_links(&v.path, &moved);
        drop(idx);
        if is_dir { store::rekey_meta(&v.data, from, to)?; }
        let mut deck_moves = moved.clone();
        if is_dir { deck_moves.insert(0, (from.to_string(), to.to_string())); }
        rekey_cards(&v.data, &deck_moves)?;
        self.emit("tree", json!({}));
        Ok(json!({ "path": to, "from": from, "isDir": is_dir, "moved": moved.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(), "updated": updated }))
    }
}

/// Flashcard decks are keyed by topic/section/notebook path; keep them attached after moves.
fn rekey_cards(data: &std::path::Path, moves: &[(String, String)]) -> std::io::Result<()> {
    let f = data.join("cards.json");
    let mut db = util::read_json(&f);
    let Some(obj) = db.as_object_mut() else { return Ok(()) };
    let mut changed = false;
    for c in obj.values_mut() {
        let Some(deck) = c["deck"].as_str().map(String::from) else { continue };
        if let Some((a, b)) = moves.iter().find(|(a, _)| under(&deck, a)) { c["deck"] = json!(format!("{b}{}", &deck[a.len()..])); changed = true; }
    }
    if changed { util::write_atomic(&f, serde_json::to_string_pretty(&db)?.as_bytes())?; }
    Ok(())
}

/// Raw folder tree of the vault (Vault files browser).
fn tree(root: &std::path::Path, dir: &std::path::Path) -> Value {
    let mut ents: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).collect();
    ents.sort_by(|a, b| {
        let (ad, bd) = (a.path().is_dir(), b.path().is_dir());
        bd.cmp(&ad).then_with(|| util::natural_cmp(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()))
    });
    Value::Array(ents.iter().map(|e| {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let rel = util::rel_of(root, &p);
        if p.is_dir() { json!({ "type": "dir", "name": name, "path": rel, "children": tree(root, &p) }) } else { json!({ "type": "file", "name": name, "path": rel }) }
    }).collect())
}

fn decode_b64(s: &str) -> Result<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = T.iter().position(|&x| x == c).ok_or_else(|| ApiError::bad_request("Invalid file data"))? as u32;
        buf = (buf << 6) | v; bits += 6;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); buf &= (1 << bits) - 1; }
    }
    Ok(out)
}
