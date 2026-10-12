//! AI features on top of the index + model providers: chat, studio tools, overviews, background indexing.
//! Prompts are carried over from the previous version so behaviour stays the same.
use crate::error::{ApiError, Result};
use crate::files::Files;
use crate::index::{esc, index_dir, SearchIndex};
use crate::layout::{self, Scope};
use crate::models::ChatOpts;
use crate::util::{basename, join, now_iso, stem};
use crate::{progress, retrieve, srs, store, Core};
use serde_json::{json, Value};
use std::sync::Arc;

pub const BRIEFS: [(&str, &str); 4] = [
    ("summary", "Write a concise briefing document: an overview paragraph, then key themes as headed sections with bullet points."),
    ("faq", "Write an FAQ: 8-12 likely questions with clear answers."),
    ("study-guide", "Write a study guide: key terms with definitions, 5 short-answer questions with answers, and 3 essay prompts."),
    ("timeline", "Write a chronological timeline of events/dates mentioned, as a bulleted list, followed by a short cast of key people/things."),
];
const CITE_RULE: &str = "Every \"cite\" must be one of the exact [[Note#Heading]] tags from the notes.";
const SYSTEM: &str = "You answer questions using ONLY the provided sources from the user's notes.
Answer in your own words. Don't copy source chunks or their headings into the answer.
After each claim, cite the source using its exact [[Note#Heading]] tag.
Sources marked (from: X) come from elsewhere in the same notebook, outside the user's current selection — mention when you rely on them.
(overview) is a high-level summary of the whole notebook: use it only as background context, and prefer the other sources for facts.
(notebook goals) describes what the user is working toward. Use it to decide what matters and how to frame the answer, but never cite it as a fact.
If the sources don't contain the answer, say so plainly.";

fn cite(c: &Value) -> String {
    let h = c["heading"].as_str().unwrap_or("");
    format!("[[{}{}]]", c["note"].as_str().unwrap_or(""), if h.is_empty() { String::new() } else { format!("#{h}") })
}
fn strs(v: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) { match v { Value::Array(a) => a.iter().for_each(|x| walk(x, out)), Value::String(s) if !s.is_empty() => out.push(s.clone()), Value::Null => {}, other if !other.is_object() => out.push(other.to_string()), _ => {} } }
    let mut out = vec![]; walk(v, &mut out); out
}
fn wrap_cite(c: &str) -> String { if c.starts_with("[[") { c.to_string() } else { format!("[[{}]]", c.trim_matches(|x| x == '[' || x == ']')) } }

impl Core {
    // ---------- index lifecycle
    pub(crate) async fn reader(&self) -> Result<Arc<SearchIndex>> {
        let v = self.vault()?;
        let dir = index_dir(&v.data, &self.models.embed_id());
        if let Some(r) = self.reader.lock().clone().filter(|r| r.dir == dir) { return Ok(r); }
        let r = Arc::new(SearchIndex::open(&dir).await?);
        *self.reader.lock() = Some(r.clone());
        Ok(r)
    }

    /// Start (or queue) a background reindex; emits "index" progress, then refreshes overviews.
    pub fn reindex(&self) {
        let Some(core) = self.me.upgrade() else { return };
        let Some(rt) = self.rt.clone() else { return };
        rt.spawn(async move {
            let _guard = core.indexing.lock().await;
            let Ok(v) = core.vault() else { return };
            core.links.lock().build(&v.path);
            let dir = index_dir(&v.data, &core.models.embed_id());
            let target = v.path.clone();
            let mut idx = match SearchIndex::open(&dir).await { Ok(i) => i, Err(e) => { eprintln!("index open failed: {}", e.message); return } };
            let emit_core = core.clone();
            let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
            let res = idx.sync(&v.path, &core.models, |done, total| {
                *emit_core.status.write() = json!({ "running": true, "done": done, "total": total });
                if last.elapsed().as_millis() > 300 || done == total { last = std::time::Instant::now(); emit_core.emit("index", json!({ "running": true, "done": done, "total": total })); }
            }, || core.vault().map(|cur| cur.path != target).unwrap_or(true)).await;
            let (total, changed, chunks, error) = match &res { Ok((t, ch, c)) => (*t, *ch, *c, Value::Null), Err(e) => (0, 0, idx.count().await, json!(e.message)) };
            *core.reader.lock() = None; // next query sees the new version
            let st = json!({ "running": false, "done": total, "total": total, "changed": changed, "chunks": chunks, "error": error });
            *core.status.write() = st.clone();
            core.emit("index", st);
            core.emit("tree", json!({}));
            if res.is_ok() { core.refresh_overviews().await; }
        });
    }

    // ---------- generation helpers
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        Ok(self.models.chat(&[json!({ "role": "user", "content": prompt })], ChatOpts::default()).await?.trim().to_string())
    }
    /// JSON-mode generation with one retry; `check` validates the parsed object.
    pub async fn generate_json(&self, prompt: &str, check: impl Fn(&Value) -> bool) -> Result<Value> {
        for attempt in 0..2 {
            let msgs = [json!({ "role": "system", "content": "Respond with valid JSON only." }), json!({ "role": "user", "content": prompt })];
            let text = self.models.chat(&msgs, ChatOpts { json: true, temperature: Some(if attempt == 1 { 0.2 } else { 0.6 }), on_token: None }).await?;
            if let Ok(v) = serde_json::from_str::<Value>(&crate::models::extract_json(&text)) { if check(&v) { return Ok(v); } }
        }
        Err(ApiError::new(502, "The model did not return valid output — try again"))
    }
    fn goals(&self, s: &Scope) -> String { self.vault().map(|v| store::goals_context(&v.data, &s.notebook, s.section.as_deref())).unwrap_or_default() }
    async fn text_for(&self, s: &Scope, max: usize) -> Result<String> {
        let text = retrieve::scope_text(&*self.reader().await?, s, max).await?;
        if text.is_empty() { return Err(ApiError::bad_request("Nothing indexed in this scope yet")); }
        let goals = self.goals(s);
        Ok(if goals.is_empty() { text } else { format!("[Learning goals: prioritize material that serves these. They are context, not notes to quote.]\n{goals}\n\n{text}") })
    }
    pub fn studio_path(s: &Scope, name: &str) -> String {
        let prefix = s.topic.as_deref().map(|t| format!("{} - ", stem(basename(t)))).unwrap_or_default();
        join(&join(&s.base(), "_studio"), &format!("{prefix}{name}.md"))
    }
    async fn sources_for(&self, question: &str, s: &Scope) -> Result<Vec<Value>> {
        let v = self.vault()?;
        retrieve::retrieve(&*self.reader().await?, &self.models, &v.path, &v.data, question, s).await
    }

    // ---------- chat (streams {token} … {sources} {followups} {done})
    pub async fn chat_stream(&self, body: &Value, on: &(dyn Fn(Value) + Send + Sync)) -> Result<()> {
        let scope: Scope = serde_json::from_value(body.clone()).unwrap_or_default();
        let question = body["question"].as_str().unwrap_or("").to_string();
        let _wait = self.indexing.lock().await; drop(_wait); // don't answer from a half-built index
        let sources = self.sources_for(&question, &scope).await?;
        let label = |c: &Value| match c["kind"].as_str() { Some("overview") => " (overview)".to_string(), Some("notebook") => format!(" (from: {})", c["origin"].as_str().unwrap_or("")), _ => String::new() };
        let goals = self.goals(&scope);
        let mut ctx = if goals.is_empty() { String::new() } else { format!("--- (notebook goals)\n{goals}\n\n") };
        ctx += &sources.iter().map(|c| format!("--- {}{}\n{}", cite(c), label(c), c["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n");
        let mut messages = vec![json!({ "role": "system", "content": SYSTEM })];
        let history: Vec<Value> = body["history"].as_array().cloned().unwrap_or_default();
        messages.extend(history.iter().skip(history.len().saturating_sub(6)).cloned());
        messages.push(json!({ "role": "user", "content": format!("Sources:\n{}\n\nQuestion: {question}", if ctx.is_empty() { "(none)".into() } else { ctx }) }));
        let mut cb = |t: &str| on(json!({ "token": t }));
        let answer = self.models.chat(&messages, ChatOpts { on_token: Some(&mut cb), ..Default::default() }).await?;
        on(json!({ "sources": sources }));
        on(json!({ "followups": self.follow_ups(&question, &answer).await }));
        Ok(())
    }
    async fn follow_ups(&self, q: &str, a: &str) -> Vec<String> {
        let a: String = a.chars().take(2000).collect();
        let r = self.generate_json(&format!("Given this Q&A from a study session, suggest 3 short follow-up questions the student might ask next.\nReturn JSON: {{\"questions\": [\"...\", \"...\", \"...\"]}}\n\nQ: {q}\nA: {a}"), |o| o["questions"].is_array()).await;
        r.map(|v| strs(&v["questions"]).into_iter().take(3).collect()).unwrap_or_default()
    }
    pub async fn chat_title(&self, q: &str, a: &str) -> Result<String> {
        let t = self.generate(&format!("Give a short, specific title (max 6 words, no quotes, no trailing period) for this Q&A.\nQ: {}\nA: {}\nTitle:", q.chars().take(500).collect::<String>(), a.chars().take(1200).collect::<String>())).await?;
        let t = t.lines().next().unwrap_or("").trim_matches(|c: char| c == '"' || c == '\'' || c == '#' || c == '*' || c == '.' || c.is_whitespace()).chars().take(80).collect::<String>();
        Ok(if t.is_empty() { "Untitled".into() } else { t })
    }

    // ---------- studio
    pub async fn brief(&self, s: &Scope, kind: &str) -> Result<Value> {
        let how = BRIEFS.iter().find(|(k, _)| *k == kind).map(|(_, h)| *h).ok_or_else(|| ApiError::bad_request("Unknown brief type"))?;
        let text = self.text_for(s, 24000).await?;
        let body = self.generate(&format!("{how}\nBase it only on these notes, citing [[Note#Heading]] tags where useful. Output Markdown only.\n\nNotes:\n{text}")).await?;
        let p = Self::studio_path(s, kind);
        Files::new(&self.vault()?.path).write(&p, &format!("---\ngenerated: {}\ntype: {kind}\n---\n\n{body}\n", now_iso()))?;
        Ok(json!({ "path": p, "body": body }))
    }

    fn deck_name(s: &Scope) -> String { s.topic.clone().or_else(|| s.section.clone()).unwrap_or_else(|| s.notebook.clone()) }
    fn mirror_deck(&self, s: &Scope) -> Result<String> {
        let v = self.vault()?;
        let name = Self::deck_name(s);
        let deck = srs::deck(&v.data, &name);
        let own: Vec<String> = deck["cards"].as_array().into_iter().flatten().filter(|c| c["deck"] == name.as_str())
            .map(|c| format!("{}::{} {}", c["front"].as_str().unwrap_or("").replace('\n', " "), c["back"].as_str().unwrap_or("").replace('\n', " "), c["cite"].as_str().unwrap_or("")).trim().to_string()).collect();
        let p = Self::studio_path(s, "Flashcards");
        Files::new(&v.path).write(&p, &format!("#flashcards\n\n{}\n", own.join("\n\n")))?;
        Ok(p)
    }
    pub async fn flashcards(&self, s: &Scope, count: u64) -> Result<Value> {
        let text = self.text_for(s, 24000).await?;
        let r = self.generate_json(&format!("Create {count} high-quality flashcards from these notes for spaced-repetition study.\nFronts are specific questions or terms; backs are concise answers (1-2 sentences). {CITE_RULE}\nReturn JSON: {{\"cards\":[{{\"front\":\"...\",\"back\":\"...\",\"cite\":\"[[Note#Heading]]\"}}]}}\n\nNotes:\n{text}"),
            |o| o["cards"].as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|c| c["front"].is_string() && c["back"].is_string()))).await?;
        let cards: Vec<Value> = r["cards"].as_array().unwrap().iter().map(|c| json!({ "front": c["front"].as_str().unwrap().trim(), "back": c["back"].as_str().unwrap().trim(), "cite": c["cite"].as_str().unwrap_or("") })).collect();
        let v = self.vault()?;
        srs::add_cards(&v.data, &Self::deck_name(s), &cards)?;
        let p = self.mirror_deck(s)?;
        let mut d = srs::deck(&v.data, &Self::deck_name(s)); d["path"] = json!(p);
        Ok(d)
    }
    pub fn add_card(&self, s: &Scope, card: &Value) -> Result<Value> {
        let (f, b) = (card["front"].as_str().unwrap_or(""), card["back"].as_str().unwrap_or(""));
        if f.is_empty() || b.is_empty() { return Err(ApiError::bad_request("Card needs a front and a back")); }
        let v = self.vault()?;
        srs::add_cards(&v.data, &Self::deck_name(s), &[json!({ "front": f, "back": b, "cite": card["cite"].as_str().unwrap_or("") })])?;
        self.mirror_deck(s)?;
        Ok(srs::deck(&v.data, &Self::deck_name(s)))
    }
    pub fn deck(&self, s: &Scope) -> Result<Value> { Ok(srs::deck(&self.vault()?.data, &Self::deck_name(s))) }
    pub fn review(&self, id: &str, grade: u8) -> Result<Value> {
        let v = self.vault()?;
        let c = srs::review(&v.data, id, grade)?.ok_or_else(|| ApiError::not_found("Unknown card"))?;
        let nb = layout::list_notebooks(&v.path, &v.data).into_iter().find(|n| crate::util::under(c["deck"].as_str().unwrap_or(""), &n.path)).map(|n| n.path);
        progress::log_event(&v.data, json!({ "kind": "card", "notebook": nb, "concept": c["cite"], "correct": grade >= 2, "label": c["front"] }))?;
        Ok(c)
    }

    pub async fn quiz(&self, s: &Scope, count: u64, difficulty: &str, given: Option<String>) -> Result<Value> {
        let text = match given { Some(t) => t, None => self.text_for(s, 24000).await? };
        let out = self.generate_json(&format!("Write a {difficulty}-difficulty multiple-choice quiz with {count} questions testing understanding (not trivia) of these notes.\nEach question has exactly 4 choices, one correct. \"answer\" is the 0-based index of the correct choice. Plausible distractors.\n\"explanation\" says why the answer is right in 1-2 sentences. {CITE_RULE}\nReturn JSON: {{\"questions\":[{{\"question\":\"...\",\"choices\":[\"a\",\"b\",\"c\",\"d\"],\"answer\":0,\"explanation\":\"...\",\"cite\":\"[[Note#Heading]]\"}}]}}\n\nNotes:\n{text}"),
            |o| o["questions"].as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|q| q["question"].is_string() && q["choices"].as_array().is_some_and(|c| c.len() == 4) && q["answer"].as_u64().is_some_and(|x| x < 4)))).await?;
        // Shuffle choices so the model's answer-position bias doesn't leak.
        let qs: Vec<Value> = out["questions"].as_array().unwrap().iter().map(|q| {
            let mut order = [0usize, 1, 2, 3];
            use rand::seq::SliceRandom; order.shuffle(&mut rand::rng());
            let choices: Vec<String> = order.iter().map(|&i| q["choices"][i].as_str().map(String::from).unwrap_or_else(|| q["choices"][i].to_string())).collect();
            let answer = order.iter().position(|&i| i as u64 == q["answer"].as_u64().unwrap()).unwrap();
            let mut q = q.clone(); q["choices"] = json!(choices); q["answer"] = json!(answer); q
        }).collect();
        Ok(json!({ "questions": qs }))
    }

    pub async fn mindmap(&self, s: &Scope) -> Result<Value> {
        let text = self.text_for(s, 24000).await?;
        let md = self.generate(&format!("Create a mind map of the key concepts in these notes as a nested Markdown outline.\nStart with a single \"# \" root title. Use \"## \" for main branches and \"- \" bullets (nested with 2 spaces) for sub-points.\nKeep each node under 8 words. At the end of leaf bullets add the relevant [[Note#Heading]] tag from the notes.\nOutput only the outline.\n\nNotes:\n{text}")).await?;
        let fence = regex::Regex::new(r"(?m)^```\w*\n?|```$").unwrap();
        let bullet_heading = regex::Regex::new(r"(?m)^(\s*)[-*]\s+#+\s*").unwrap();
        let md = normalize_outline(bullet_heading.replace_all(&fence.replace_all(&md, ""), "$1- ").trim());
        let p = Self::studio_path(s, "Mind Map");
        Files::new(&self.vault()?.path).write(&p, &format!("---\nmindmap-plugin: basic\n---\n\n{md}\n"))?;
        Ok(json!({ "path": p, "markdown": md }))
    }

    pub async fn explain_back(&self, s: &Scope, subject: &str, explanation: &str) -> Result<Value> {
        let mut fs = s.clone(); fs.mode = Some("focused".into());
        let sources = self.sources_for(&format!("{subject}\n{explanation}"), &fs).await?;
        if sources.is_empty() { return Err(ApiError::bad_request("No notes found for this topic")); }
        let ctx = sources.iter().map(|c| format!("--- {}\n{}", cite(c), c["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n");
        let goals = self.goals(s);
        let goals_part = if goals.is_empty() { String::new() } else { format!("The student's goals for this material:\n{goals}\nWeigh gaps that matter for these goals more heavily.\n\n") };
        let r = self.generate_json(&format!("{goals_part}You are a supportive tutor using the Feynman technique. The student explained \"{subject}\" in their own words.\nGrade their explanation ONLY against the reference notes. Be specific and encouraging.\nReturn JSON: {{\"score\": 0-10, \"summary\": \"one-sentence verdict\", \"gotRight\": [\"...\"], \"gaps\": [\"important things from the notes they left out\"], \"misconceptions\": [\"things they said that contradict the notes\"], \"cites\": [\"[[Note#Heading]]\"]}}\n\nReference notes:\n{ctx}\n\nStudent's explanation:\n{explanation}"), |o| o["score"].is_number()).await?;
        let cites: Vec<String> = strs(&r["cites"]).iter().map(|c| wrap_cite(c)).collect();
        let concept = cites.first().cloned().unwrap_or_else(|| cite(&sources[0]));
        let score = r["score"].as_f64().unwrap_or(0.0);
        progress::log_event(&self.vault()?.data, json!({ "kind": "explain", "notebook": s.notebook, "concept": concept, "score": (score / 10.0).clamp(0.0, 1.0), "correct": score >= 7.0, "label": subject }))?;
        let mut out = r.clone();
        out["gotRight"] = json!(strs(&r["gotRight"])); out["gaps"] = json!(strs(&r["gaps"])); out["misconceptions"] = json!(strs(&r["misconceptions"]));
        out["cites"] = json!(cites); out["sources"] = json!(sources);
        Ok(out)
    }

    /// Targeted practice: quiz questions drawn only from the weak [[Note#Heading]] chunks.
    pub async fn drill(&self, s: &Scope, concepts: &[String], count: u64) -> Result<Value> {
        let idx = self.reader().await?;
        let re = regex::Regex::new(r"\[\[([^\]|#]+)(?:#([^\]|]+))?").unwrap();
        let mut parts = vec![];
        for c in concepts.iter().take(6) {
            let Some(m) = re.captures(c) else { continue };
            let mut filter = format!("note = '{}'", esc(m[1].trim()));
            if let Some(h) = m.get(2) { filter += &format!(" AND heading = '{}'", esc(h.as_str().trim())); }
            for r in idx.rows(Some(&filter), false, Some(3)).await? {
                parts.push(format!("### [[{}{}]]\n{}", r.note, if r.heading.is_empty() { String::new() } else { format!("#{}", r.heading) }, r.text));
            }
        }
        if parts.is_empty() { return Err(ApiError::bad_request("Couldn’t find the notes behind these weak spots")); }
        self.quiz(s, count, "medium", Some(parts.join("\n\n"))).await
    }

    /// Inline AI on an editor selection.
    pub async fn assist(&self, action: &str, text: &str, s: &Scope) -> Result<Value> {
        let text: String = text.chars().take(4000).collect();
        if text.trim().is_empty() { return Err(ApiError::bad_request("Select some text first")); }
        match action {
            "flashcard" => {
                let c = self.generate_json(&format!("Turn this passage into ONE high-quality flashcard. Return JSON {{\"front\":\"question\",\"back\":\"concise answer\"}}.\n\nPassage:\n{text}"), |o| o["front"].is_string() && o["back"].is_string()).await?;
                Ok(json!({ "card": { "front": c["front"], "back": c["back"] } }))
            }
            "quiz" => {
                let q = self.generate_json(&format!("Write ONE multiple-choice question testing understanding of this passage. 4 choices, \"answer\" is the 0-based index.\nReturn JSON {{\"question\":\"...\",\"choices\":[\"a\",\"b\",\"c\",\"d\"],\"answer\":0,\"explanation\":\"...\"}}\n\nPassage:\n{text}"),
                    |o| o["question"].is_string() && o["choices"].as_array().is_some_and(|c| c.len() == 4) && o["answer"].is_u64()).await?;
                Ok(json!({ "question": q }))
            }
            "explain" | "simplify" => {
                let mut fs = s.clone(); fs.mode = Some("focused".into());
                let ctx = if s.notebook.is_empty() { String::new() } else {
                    self.sources_for(&text, &fs).await.unwrap_or_default().iter().take(5).map(|c| format!("--- {}\n{}", cite(c), c["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n")
                };
                let how = if action == "explain" { "Explain this passage clearly for a student, in 2-4 short paragraphs. Use the related notes for context and cite them as [[Note#Heading]] when you use them." } else { "Rewrite this passage in simpler, plainer language. Keep every fact. Output only the rewritten text." };
                Ok(json!({ "text": self.generate(&format!("{how}\n\nPassage:\n{text}\n\nRelated notes:\n{}", if ctx.is_empty() { "(none)" } else { &ctx })).await? }))
            }
            _ => Err(ApiError::bad_request(format!("Unknown action: {action}"))),
        }
    }

    pub async fn suggest_links(&self, note_path: &str, notebook: &str) -> Result<Value> {
        let idx = self.reader().await?;
        let already: std::collections::HashSet<String> = self.links.lock().links_of(note_path).links.into_iter().collect();
        let cands: Vec<Value> = retrieve::related(&idx, note_path, Some(notebook), 8).await?.into_iter()
            .filter(|r| !already.contains(r["path"].as_str().unwrap_or("")) && r["score"].as_f64().unwrap_or(0.0) >= 0.45).take(5).collect();
        if cands.is_empty() { return Ok(json!([])); }
        let topic = |p: &str| Scope { topic: Some(p.to_string()), ..Default::default() };
        let own: String = retrieve::scope_text(&idx, &topic(note_path), 3000).await?.chars().take(3000).collect();
        let mut others = vec![];
        for (i, c) in cands.iter().enumerate() {
            let t = retrieve::scope_text(&idx, &topic(c["path"].as_str().unwrap_or("")), 600).await?;
            others.push(format!("[{i}] {}: {}", c["note"].as_str().unwrap_or(""), t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect::<String>()));
        }
        let r = self.generate_json(&format!("A student is writing the note below. For each candidate note, say whether linking to it would help, and why in one short sentence.\nReturn JSON {{\"links\":[{{\"i\":0,\"useful\":true,\"reason\":\"...\"}}]}}\n\nNote:\n{own}\n\nCandidates:\n{}", others.join("\n")), |o| o["links"].is_array()).await
            .unwrap_or_else(|_| json!({ "links": (0..cands.len()).map(|i| json!({ "i": i, "useful": true, "reason": "" })).collect::<Vec<_>>() }));
        Ok(Value::Array(r["links"].as_array().into_iter().flatten().filter(|l| l["useful"] != false).filter_map(|l| {
            let mut c = cands.get(l["i"].as_u64()? as usize)?.clone(); c["reason"] = json!(l["reason"].as_str().unwrap_or("")); Some(c)
        }).collect()))
    }

    // ---------- notebook overviews
    async fn refresh_overviews(&self) {
        let Ok(v) = self.vault() else { return };
        let Ok(idx) = self.reader().await else { return };
        for nb in layout::list_notebooks(&v.path, &v.data) {
            if nb.topic_count() < 2 || self.vault().map(|c| c.path != v.path).unwrap_or(true) { continue; }
            let rows = idx.rows(Some(&crate::index::under_sql(&nb.path)), false, None).await.unwrap_or_default();
            if rows.is_empty() { continue; }
            let mut parts: Vec<String> = rows.iter().map(|r| format!("{}:{}", r.path, r.mtime)).collect(); parts.sort(); parts.dedup();
            let goals = store::goals_context(&v.data, &nb.path, None);
            let sig = format!("{:016x}", fnv(&(parts.join("\n") + "\n" + &goals)));
            let path = retrieve::overview_path(&nb.path);
            let existing = std::fs::read_to_string(crate::util::join_rel(&v.path, &path)).unwrap_or_default();
            if existing.contains(&format!("signature: {sig}")) { continue; }
            *self.overview_running.write() = Some(nb.path.clone());
            self.emit("overview", json!({ "running": nb.path }));
            let text = retrieve::scope_text(&idx, &Scope { notebook: nb.path.clone(), ..Default::default() }, 20000).await.unwrap_or_default();
            let sections = if nb.sections.is_empty() { "(none)".to_string() } else { nb.sections.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join(", ") };
            let framing = if goals.is_empty() { String::new() } else { format!("The user describes this notebook like this (use it to frame the overview, not as a source of facts):\n{goals}\n") };
            let prompt = format!("Write a high-level overview of this notebook titled \"{}\" in 120-220 words of Markdown.\nStart with 2-3 sentences on its overall focus and goal. Then a \"Sections\" list with one line per section saying what it covers.\nSections: {sections}\n{framing}Be factual and only use the notes below.\n\nNotes:\n{text}", nb.name);
            match self.generate(&prompt).await {
                Ok(body) => { let _ = Files::new(&v.path).write(&path, &format!("---\nsignature: {sig}\ngenerated: {}\n---\n\n{body}\n", now_iso())); }
                Err(e) => eprintln!("overview failed for {}: {}", nb.path, e.message),
            }
        }
        *self.overview_running.write() = None;
        self.emit("overview", json!({ "running": null }));
    }
}

fn fnv(s: &str) -> u64 { s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)) }

/// Headings flush-left; bullets re-indented relative to the first bullet after each heading
/// (models often indent 4+ spaces, which Markdown would turn into a code block).
fn normalize_outline(md: &str) -> String {
    let bullet = regex::Regex::new(r"^(\s*)[-*+]\s+(.*)").unwrap();
    let mut base: Option<usize> = None;
    md.split('\n').map(|line| {
        if line.trim_start().starts_with('#') { base = None; return line.trim().to_string(); }
        let Some(m) = bullet.captures(line) else { return line.trim().to_string() };
        let indent = m[1].len();
        let b = *base.get_or_insert(indent);
        let depth = ((indent as f64 - b as f64) / 2.0).round().max(0.0) as usize;
        format!("{}- {}", "  ".repeat(depth), &m[2])
    }).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outline() { assert_eq!(normalize_outline("# Root\n## A\n    - x\n      - y\n    - z"), "# Root\n## A\n- x\n  - y\n- z"); }
    #[test]
    fn studio_paths() {
        assert_eq!(Core::studio_path(&Scope { notebook: "P".into(), topic: Some("P/S/T.md".into()), ..Default::default() }, "Quiz"), "P/S/_studio/T - Quiz.md");
        assert_eq!(Core::studio_path(&Scope { notebook: "P".into(), section: Some("P/S".into()), ..Default::default() }, "summary"), "P/S/_studio/summary.md");
    }
}
