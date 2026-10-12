//! Model providers behind one interface: local Ollama, Anthropic (Claude, raw HTTP — there is no
//! official Rust SDK), and any OpenAI-compatible server. Settings live in <data>/models.json (app-wide);
//! API keys live in the OS keychain (Keychain / Credential Manager / Secret Service), never in files.
use crate::error::{ApiError, Result};
use crate::util;
use futures::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pick { pub provider: String, pub model: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub chat: Pick,
    pub embed: Pick,
    #[serde(default)] pub ollama: OllamaCfg,
    #[serde(default)] pub anthropic: AnthropicCfg,
    #[serde(default)] pub openai: OpenAiCfg,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OllamaCfg { pub base_url: String }
impl Default for OllamaCfg { fn default() -> Self { Self { base_url: "http://127.0.0.1:11434".into() } } }
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnthropicCfg { pub effort: String }
impl Default for AnthropicCfg { fn default() -> Self { Self { effort: "medium".into() } } }
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCfg { pub base_url: String, pub label: String }
impl Default for OpenAiCfg { fn default() -> Self { Self { base_url: "https://api.openai.com/v1".into(), label: "OpenAI-compatible".into() } } }
impl Default for Config {
    fn default() -> Self {
        Config {
            chat: Pick { provider: "ollama".into(), model: std::env::var("CHAT_MODEL").unwrap_or_else(|_| "llama3.1:8b".into()) },
            embed: Pick { provider: "ollama".into(), model: std::env::var("EMBED_MODEL").unwrap_or_else(|_| "nomic-embed-text".into()) },
            ollama: Default::default(), anthropic: Default::default(), openai: Default::default(),
        }
    }
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| reqwest::Client::builder().user_agent(concat!("Vault-LM/", env!("CARGO_PKG_VERSION"))).build().unwrap());

pub struct Models { file: PathBuf, cfg: RwLock<Config> }

// ---------- keys (keychain, with env fallback; VAULTLM_KEYS_IN_FILE=1 for tests)
const KEYCHAIN_SERVICE: &str = "com.vaultlm.app";
fn key_file(dir: &Path) -> PathBuf { dir.join("keys.test.json") }
fn keys_in_file() -> bool { std::env::var("VAULTLM_KEYS_IN_FILE").is_ok() }

impl Models {
    pub fn new(data_root: &Path) -> Models {
        let file = data_root.join("models.json");
        let cfg = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Models { file, cfg: RwLock::new(cfg) }
    }
    pub fn config(&self) -> Config { self.cfg.read().clone() }
    fn save(&self) -> Result<()> {
        util::write_atomic(&self.file, serde_json::to_string_pretty(&*self.cfg.read())?.as_bytes())?;
        Ok(())
    }

    pub fn key(&self, provider: &str) -> String {
        let stored = if keys_in_file() {
            util::read_json(&key_file(self.file.parent().unwrap()))[provider].as_str().unwrap_or("").to_string()
        } else {
            keyring::Entry::new(KEYCHAIN_SERVICE, provider).and_then(|e| e.get_password()).unwrap_or_default()
        };
        if !stored.is_empty() { return stored; }
        std::env::var(if provider == "anthropic" { "ANTHROPIC_API_KEY" } else { "OPENAI_API_KEY" }).unwrap_or_default()
    }
    fn set_key(&self, provider: &str, key: &str) -> Result<()> {
        if keys_in_file() {
            let f = key_file(self.file.parent().unwrap());
            let mut m = util::read_json(&f); m[provider] = json!(key);
            util::write_atomic(&f, m.to_string().as_bytes())?;
            return Ok(());
        }
        let e = keyring::Entry::new(KEYCHAIN_SERVICE, provider).map_err(|e| ApiError::new(500, format!("Keychain unavailable: {e}")))?;
        if key.is_empty() { let _ = e.delete_credential(); } else { e.set_password(key).map_err(|e| ApiError::new(500, format!("Couldn’t save the key to the keychain: {e}")))?; }
        Ok(())
    }
    fn key_from_env(&self, provider: &str) -> bool {
        let stored = if keys_in_file() { util::read_json(&key_file(self.file.parent().unwrap()))[provider].as_str().is_some_and(|s| !s.is_empty()) }
            else { keyring::Entry::new(KEYCHAIN_SERVICE, provider).and_then(|e| e.get_password()).is_ok() };
        !stored && !self.key(provider).is_empty()
    }

    pub fn public_config(&self) -> Value {
        let c = self.config();
        let mask = |k: String| if k.is_empty() { String::new() } else { format!("{}…{}", &k[..k.len().min(7)], &k[k.len().saturating_sub(4)..]) };
        json!({
            "chat": c.chat, "embed": c.embed, "ollama": { "baseUrl": c.ollama.base_url },
            "anthropic": { "effort": c.anthropic.effort, "hasKey": !self.key("anthropic").is_empty(), "keyHint": mask(self.key("anthropic")), "fromEnv": self.key_from_env("anthropic") },
            "openai": { "baseUrl": c.openai.base_url, "label": c.openai.label, "hasKey": !self.key("openai").is_empty(), "keyHint": mask(self.key("openai")), "fromEnv": self.key_from_env("openai") },
        })
    }

    /// Apply a UI patch: { chat?, embed?, ollama?, anthropic?, openai? }; `apiKey: null` keeps the current key.
    pub fn update(&self, patch: &Value) -> Result<Value> {
        {
            let mut c = self.cfg.write();
            if let Some(p) = patch.get("chat") { merge_pick(&mut c.chat, p); }
            if let Some(p) = patch.get("embed") { merge_pick(&mut c.embed, p); }
            if let Some(u) = patch.pointer("/ollama/baseUrl").and_then(|v| v.as_str()) { c.ollama.base_url = u.trim().trim_end_matches('/').to_string(); }
            if let Some(e) = patch.pointer("/anthropic/effort").and_then(|v| v.as_str()) { c.anthropic.effort = e.into(); }
            if let Some(u) = patch.pointer("/openai/baseUrl").and_then(|v| v.as_str()) { c.openai.base_url = u.trim().trim_end_matches('/').to_string(); }
            if let Some(l) = patch.pointer("/openai/label").and_then(|v| v.as_str()) { c.openai.label = l.into(); }
        }
        for p in ["anthropic", "openai"] {
            if let Some(k) = patch.pointer(&format!("/{p}/apiKey")).and_then(|v| v.as_str()) { self.set_key(p, k.trim())?; }
        }
        self.save()?;
        Ok(self.public_config())
    }

    pub fn embed_id(&self) -> String { let c = self.config(); format!("{}:{}", c.embed.provider, c.embed.model.trim_end_matches(":latest")) }
    pub fn chat_label(&self) -> String {
        let c = self.config();
        match c.chat.provider.as_str() { "ollama" => c.chat.model, "anthropic" => format!("{} (Anthropic)", c.chat.model), _ => format!("{} ({})", c.chat.model, c.openai.label) }
    }
    pub fn is_cloud(&self) -> bool {
        let c = self.config();
        match c.chat.provider.as_str() { "anthropic" => true, "openai" => !["localhost", "127.0.0.1", "0.0.0.0"].iter().any(|h| c.openai.base_url.contains(h)), _ => false }
    }

    // ---------- chat
    /// messages: [{role: system|user|assistant, content}] → full text; `on_token` receives streamed text.
    pub async fn chat(&self, messages: &[Value], opts: ChatOpts<'_>) -> Result<String> {
        let c = self.config();
        let text = match c.chat.provider.as_str() {
            "anthropic" => self.anthropic_chat(&c, messages, opts).await?,
            "openai" => self.openai_chat(&c, messages, opts).await?,
            _ => ollama_chat(&c, messages, opts).await?,
        };
        Ok(text)
    }

    async fn anthropic_chat(&self, c: &Config, messages: &[Value], mut opts: ChatOpts<'_>) -> Result<String> {
        let key = self.key("anthropic");
        if key.is_empty() { return Err(ApiError::bad_request("No Anthropic API key yet. Add one in Models (or set ANTHROPIC_API_KEY)")); }
        let mut system: String = messages.iter().filter(|m| m["role"] == "system").filter_map(|m| m["content"].as_str()).collect::<Vec<_>>().join("\n\n");
        if opts.json { system += "\n\nRespond with a single valid JSON object and nothing else."; }
        let turns: Vec<Value> = messages.iter().filter(|m| m["role"] != "system").map(|m| json!({ "role": m["role"], "content": m["content"] })).collect();
        // Newer models reject `temperature` and assistant prefill, so neither is sent. Streaming avoids timeouts.
        let mut body = json!({ "model": c.chat.model, "max_tokens": 64000, "system": system, "messages": turns, "stream": true, "output_config": { "effort": c.anthropic.effort } });
        let mut req = HTTP.post("https://api.anthropic.com/v1/messages").header("x-api-key", key).header("anthropic-version", "2023-06-01");
        if fallback_model(&c.chat.model) {
            // Server-side refusal fallback: a declined request is re-run on Anthropic's recommended model.
            req = req.header("anthropic-beta", "server-side-fallback-2026-07-01");
            body["fallbacks"] = json!("default");
        }
        let res = req.json(&body).send().await.map_err(|e| ApiError::new(502, format!("Couldn’t reach Anthropic ({e})")))?;
        if !res.status().is_success() { return Err(anthropic_status(res).await); }
        let (mut text, mut stop) = (String::new(), String::new());
        sse_lines(res, |event, data| {
            let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(()) };
            match v["type"].as_str().unwrap_or(event) {
                "content_block_delta" if v["delta"]["type"] == "text_delta" => {
                    let t = v["delta"]["text"].as_str().unwrap_or("");
                    text.push_str(t);
                    if let Some(cb) = opts.on_token.as_mut() { cb(t); }
                }
                "message_delta" => { if let Some(s) = v["delta"]["stop_reason"].as_str() { stop = s.into(); } }
                "error" => return Err(ApiError::new(502, format!("Anthropic error: {}", v["error"]["message"].as_str().unwrap_or("unknown")))),
                _ => {}
            }
            Ok(())
        }).await?;
        if stop == "refusal" { return Err(ApiError::new(422, "The model declined to answer this request.")); }
        Ok(if opts.json { extract_json(&text) } else { text })
    }

    async fn openai_chat(&self, c: &Config, messages: &[Value], mut opts: ChatOpts<'_>) -> Result<String> {
        let mut body = json!({ "model": c.chat.model, "messages": messages, "stream": true });
        if let Some(t) = opts.temperature { body["temperature"] = json!(t); }
        if opts.json { body["response_format"] = json!({ "type": "json_object" }); }
        let mut res = self.openai_req(c, "/chat/completions", Some(&body)).await;
        if let (Err(e), true) = (&res, opts.json) {
            if e.message.contains("response_format") { body.as_object_mut().unwrap().remove("response_format"); res = self.openai_req(c, "/chat/completions", Some(&body)).await; }
        }
        let mut text = String::new();
        sse_lines(res?, |_, data| {
            if data == "[DONE]" { return Ok(()); }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(t) = v["choices"][0]["delta"]["content"].as_str() { text.push_str(t); if let Some(cb) = opts.on_token.as_mut() { cb(t); } }
            }
            Ok(())
        }).await?;
        Ok(if opts.json { extract_json(&text) } else { text })
    }

    async fn openai_req(&self, c: &Config, path: &str, body: Option<&Value>) -> Result<reqwest::Response> {
        let url = format!("{}{path}", c.openai.base_url.trim_end_matches('/'));
        let mut req = match body { Some(b) => HTTP.post(&url).json(b), None => HTTP.get(&url) };
        let key = self.key("openai");
        if !key.is_empty() { req = req.bearer_auth(key); }
        let res = req.send().await.map_err(|e| ApiError::new(502, format!("Couldn’t reach {} ({})", c.openai.base_url, short_err(&e))))?;
        if !res.status().is_success() {
            let status = res.status().as_u16();
            let t = res.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"]["message"].as_str().map(String::from)).unwrap_or(t);
            return Err(ApiError::new(502, format!("{} error {status}: {}", c.openai.label, msg.chars().take(300).collect::<String>())));
        }
        Ok(res)
    }

    // ---------- embeddings (Anthropic has none)
    pub async fn embed(&self, texts: &[String], kind: &str) -> Result<Vec<Vec<f32>>> {
        let c = self.config();
        // nomic-embed models expect task prefixes; other models don't.
        let input: Vec<String> = if c.embed.model.to_lowercase().contains("nomic") { texts.iter().map(|t| format!("{kind}: {t}")).collect() } else { texts.to_vec() };
        let vecs: Vec<Vec<f32>> = if c.embed.provider == "openai" {
            let v: Value = self.openai_req(&c, "/embeddings", Some(&json!({ "model": c.embed.model, "input": input }))).await?.json().await.map_err(http_err)?;
            let mut data: Vec<Value> = v["data"].as_array().cloned().unwrap_or_default();
            data.sort_by_key(|d| d["index"].as_u64().unwrap_or(0));
            data.iter().map(|d| floats(&d["embedding"])).collect()
        } else {
            let res = HTTP.post(format!("{}/api/embed", c.ollama.base_url)).json(&json!({ "model": c.embed.model, "input": input })).send().await.map_err(ollama_down)?;
            if !res.status().is_success() { return Err(ApiError::new(502, format!("Ollama embed error: {}", res.text().await.unwrap_or_default()))); }
            let v: Value = res.json().await.map_err(http_err)?;
            v["embeddings"].as_array().into_iter().flatten().map(floats).collect()
        };
        if vecs.len() != texts.len() { return Err(ApiError::new(502, "Embedding service returned the wrong number of vectors")); }
        Ok(vecs)
    }

    // ---------- discovery
    pub async fn list(&self, provider: &str) -> Result<Vec<Value>> {
        let c = self.config();
        let embed_hint = regex::Regex::new(r"(?i)embed|bge|e5-|gte|minilm|mxbai|nomic-bert|arctic-embed").unwrap();
        match provider {
            "ollama" => {
                let v: Value = HTTP.get(format!("{}/api/tags", c.ollama.base_url)).send().await.map_err(ollama_down)?.json().await.map_err(http_err)?;
                let mut out: Vec<Value> = v["models"].as_array().into_iter().flatten().map(|m| {
                    let name = m["name"].as_str().unwrap_or("");
                    let embed = embed_hint.is_match(name) || m["details"]["family"].as_str().unwrap_or("").to_lowercase().contains("bert");
                    json!({ "id": name, "name": name, "size": m["size"], "params": m["details"]["parameter_size"], "quant": m["details"]["quantization_level"], "kind": if embed { "embed" } else { "chat" } })
                }).collect();
                out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                Ok(out)
            }
            "anthropic" => {
                let key = self.key("anthropic");
                if key.is_empty() { return Err(ApiError::bad_request("No Anthropic API key yet. Add one in Models (or set ANTHROPIC_API_KEY)")); }
                let mut out = vec![];
                let mut after: Option<String> = None;
                loop {
                    let mut url = "https://api.anthropic.com/v1/models?limit=100".to_string();
                    if let Some(a) = &after { url += &format!("&after_id={a}"); }
                    let res = HTTP.get(&url).header("x-api-key", &key).header("anthropic-version", "2023-06-01").send().await.map_err(|e| ApiError::new(502, format!("Couldn’t reach Anthropic ({e})")))?;
                    if !res.status().is_success() { return Err(anthropic_status(res).await); }
                    let v: Value = res.json().await.map_err(http_err)?;
                    for m in v["data"].as_array().into_iter().flatten() {
                        out.push(json!({ "id": m["id"], "name": m["display_name"].as_str().or(m["id"].as_str()), "kind": "chat", "context": m["max_input_tokens"], "created": m["created_at"] }));
                    }
                    if v["has_more"].as_bool() != Some(true) { break; }
                    after = v["last_id"].as_str().map(String::from);
                }
                Ok(out)
            }
            _ => {
                let v: Value = self.openai_req(&c, "/models", None).await?.json().await.map_err(http_err)?;
                let mut out: Vec<Value> = v["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str()).map(|id| json!({ "id": id, "name": id, "kind": if embed_hint.is_match(id) { "embed" } else { "chat" } })).collect();
                out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
                Ok(out)
            }
        }
    }

    /// Download a local model, reporting progress as (status, completed, total).
    pub async fn pull(&self, name: &str, mut progress: impl FnMut(&str, u64, u64)) -> Result<()> {
        let c = self.config();
        let res = HTTP.post(format!("{}/api/pull", c.ollama.base_url)).json(&json!({ "model": name, "stream": true })).send().await.map_err(ollama_down)?;
        let mut stream = res.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk.map_err(http_err)?));
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                    if let Some(e) = v["error"].as_str() { return Err(ApiError::new(502, e.to_string())); }
                    progress(v["status"].as_str().unwrap_or(""), v["completed"].as_u64().unwrap_or(0), v["total"].as_u64().unwrap_or(1));
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct ChatOpts<'a> { pub json: bool, pub temperature: Option<f32>, pub on_token: Option<&'a mut (dyn FnMut(&str) + Send)> }

fn merge_pick(p: &mut Pick, v: &Value) {
    if let Some(x) = v["provider"].as_str() { p.provider = x.into(); }
    if let Some(x) = v["model"].as_str() { p.model = x.into(); }
}
fn fallback_model(m: &str) -> bool { matches!(m, "claude-fable-5-1" | "claude-opus-5-5" | "claude-opus-5" | "claude-sonnet-5-5") }
fn floats(v: &Value) -> Vec<f32> { v.as_array().into_iter().flatten().filter_map(|x| x.as_f64()).map(|x| x as f32).collect() }
fn http_err(e: reqwest::Error) -> ApiError { ApiError::new(502, e.to_string()) }
fn short_err(e: &reqwest::Error) -> String { if e.is_connect() { "connection refused".into() } else if e.is_timeout() { "timed out".into() } else { e.to_string() } }
fn ollama_down(e: reqwest::Error) -> ApiError {
    if e.is_connect() { ApiError::new(503, "Ollama isn’t running. Start it (ollama serve) or choose another provider in Models.") } else { http_err(e) }
}
async fn anthropic_status(res: reqwest::Response) -> ApiError {
    let status = res.status().as_u16();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    let msg = body["error"]["message"].as_str().unwrap_or("").to_string();
    ApiError::new(if status == 401 { 400 } else { 502 }, match status {
        401 => "Anthropic rejected the API key".to_string(),
        403 => "This API key can’t use that model".to_string(),
        404 => "Model not found on Anthropic".to_string(),
        429 => "Anthropic rate limit reached. Try again shortly".to_string(),
        _ => format!("Anthropic error {status}: {msg}"),
    })
}

/// Read a text/event-stream (or NDJSON-free SSE) response, calling `f(event, data)` per message.
async fn sse_lines(res: reqwest::Response, mut f: impl FnMut(&str, &str) -> Result<()>) -> Result<()> {
    let mut stream = res.bytes_stream();
    let (mut buf, mut event) = (String::new(), String::new());
    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk.map_err(http_err)?));
        while let Some(i) = buf.find('\n') {
            let line: String = buf.drain(..=i).collect();
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(e) = line.strip_prefix("event:") { event = e.trim().to_string(); }
            else if let Some(d) = line.strip_prefix("data:") { f(&event, d.trim())?; }
            else if line.is_empty() { event.clear(); }
        }
    }
    Ok(())
}

async fn ollama_chat(c: &Config, messages: &[Value], mut opts: ChatOpts<'_>) -> Result<String> {
    let mut options = json!({ "num_ctx": 16384 });
    if let Some(t) = opts.temperature { options["temperature"] = json!(t); }
    let mut body = json!({ "model": c.chat.model, "messages": messages, "stream": true, "options": options });
    if opts.json { body["format"] = json!("json"); }
    let res = HTTP.post(format!("{}/api/chat", c.ollama.base_url)).json(&body).send().await.map_err(ollama_down)?;
    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"].as_str().map(String::from)).unwrap_or(t);
        return Err(ApiError::new(502, format!("Ollama: {msg}")));
    }
    let mut stream = res.bytes_stream();
    let (mut buf, mut text) = (String::new(), String::new());
    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk.map_err(http_err)?));
        while let Some(i) = buf.find('\n') {
            let line: String = buf.drain(..=i).collect();
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                if let Some(e) = v["error"].as_str() { return Err(ApiError::new(502, format!("Ollama: {e}"))); }
                if let Some(t) = v["message"]["content"].as_str() { text.push_str(t); if let Some(cb) = opts.on_token.as_mut() { cb(t); } }
            }
        }
    }
    Ok(text)
}

/// Some providers wrap JSON in prose or fences; pull out the first complete object.
pub fn extract_json(text: &str) -> String {
    let t = text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let Some(start) = t.find('{') else { return t.to_string() };
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, ch) in t[start..].char_indices() {
        if in_str { if esc { esc = false } else if ch == '\\' { esc = true } else if ch == '"' { in_str = false } continue; }
        match ch { '"' => in_str = true, '{' => depth += 1, '}' => { depth -= 1; if depth == 0 { return t[start..start + i + 1].to_string(); } } _ => {} }
    }
    t[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_extraction() {
        assert_eq!(extract_json("Sure! ```json\n{\"a\": {\"b\": \"}\"}}\n``` hope that helps"), "{\"a\": {\"b\": \"}\"}}");
    }
    #[test]
    fn config_roundtrip_and_keys_never_in_config() {
        std::env::set_var("VAULTLM_KEYS_IN_FILE", "1");
        let d = tempfile::tempdir().unwrap();
        let m = Models::new(d.path());
        let pub_ = m.update(&json!({ "chat": { "provider": "anthropic", "model": "claude-opus-5-5" }, "anthropic": { "apiKey": "sk-ant-abcdefgh1234" } })).unwrap();
        assert_eq!(pub_["anthropic"]["keyHint"], "sk-ant-…1234");
        let on_disk = std::fs::read_to_string(d.path().join("models.json")).unwrap();
        assert!(!on_disk.contains("sk-ant"), "keys must not be written to models.json");
        assert!(m.is_cloud());
        assert_eq!(Models::new(d.path()).config().chat.model, "claude-opus-5-5");
    }
}
