//! Import outside material as Markdown sources: web pages, YouTube transcripts, PDFs (layout-aware,
//! OCR for scans), EPUBs, images (OCR) and plain text/HTML. Everything is converted locally.
//! The original file is kept beside the source note so it can be opened or re-converted later.
use crate::error::{ApiError, Result};
use crate::files::Files;
use crate::util::{basename, dirname, ext, join, now_iso, stem};
use crate::{pdfmd, Core};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn safe_title(s: &str) -> String {
    let t: String = s.chars().filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']')).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(100).collect();
    if t.is_empty() { "Untitled".into() } else { t }
}
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
}
fn to_markdown(html: &str) -> String {
    htmd::HtmlToMarkdown::builder().skip_tags(vec!["script", "style", "img", "svg", "iframe", "noscript"]).build().convert(html).unwrap_or_default()
}
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
fn http() -> reqwest::Client { reqwest::Client::builder().user_agent(UA).build().unwrap() }
fn net(e: reqwest::Error) -> ApiError { ApiError::new(502, format!("Download failed: {e}")) }

impl Core {
    fn job(&self, id: &str, label: &str, done: usize, total: usize, note: &str) { self.emit("job", json!({ "id": id, "label": label, "done": done, "total": total.max(1), "note": note })); }
    fn job_end(&self, id: &str, label: &str) { self.emit("job", json!({ "id": id, "label": label, "finished": true })); }

    fn save_source(&self, dir: &str, title: &str, meta: &[(&str, Value)], body: &str) -> Result<String> {
        let v = self.vault()?;
        let rel = join(dir, &format!("{}.md", safe_title(title)));
        let mut fm = String::new();
        for (k, val) in meta.iter().filter(|(_, v)| !v.is_null() && v.as_str() != Some("")) { fm += &format!("{k}: {val}\n"); }
        fm += &format!("added: \"{}\"\n", now_iso());
        Files::new(&v.path).write(&rel, &format!("---\n{fm}---\n\n# {title}\n\n{}\n", body.trim()))?;
        self.links.lock().build(&v.path);
        Ok(rel)
    }

    // ---------- URLs
    pub async fn add_url(&self, dir: &str, url: &str) -> Result<String> {
        if let Some(id) = youtube_id(url) { return self.add_youtube(dir, &id).await; }
        let res = http().get(url).send().await.map_err(net)?;
        if !res.status().is_success() { return Err(ApiError::new(502, format!("Fetch failed: {}", res.status()))); }
        let is_pdf = res.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.contains("application/pdf"));
        if is_pdf {
            let name = url.rsplit('/').next().filter(|n| n.ends_with(".pdf")).unwrap_or("document.pdf").to_string();
            let bytes = res.bytes().await.map_err(net)?;
            return self.add_file(dir, &name, &bytes, Some(url), None).await;
        }
        let html = res.text().await.map_err(net)?;
        let (title, content) = match dom_smoothie::Readability::new(html.as_str(), Some(url), None).and_then(|mut r| r.parse()) {
            Ok(a) => (a.title.to_string(), a.content.to_string()),
            Err(_) => return Err(ApiError::bad_request("Could not extract article content")),
        };
        self.save_source(dir, if title.is_empty() { url } else { &title }, &[("source", json!(url)), ("type", json!("web"))], &to_markdown(&content))
    }

    async fn add_youtube(&self, dir: &str, id: &str) -> Result<String> {
        let (tracks, title, author) = youtube_tracks(id).await?;
        let track = tracks.iter().find(|t| t["languageCode"].as_str().is_some_and(|l| l.starts_with("en"))).or(tracks.first()).cloned()
            .ok_or_else(|| ApiError::bad_request("No transcript available for this video"))?;
        let url = track["baseUrl"].as_str().unwrap_or("");
        if !reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.ends_with(".youtube.com"))).unwrap_or(false) {
            return Err(ApiError::bad_request("No transcript available for this video"));
        }
        let xml = http().get(url).send().await.map_err(net)?.text().await.map_err(net)?;
        let items = parse_transcript(&xml);
        if items.is_empty() { return Err(ApiError::bad_request("No transcript available for this video")); }
        // ~60-second paragraphs with timestamps.
        let mut blocks: Vec<(f64, Vec<String>)> = vec![];
        for (ms, text) in items {
            if blocks.last().map_or(true, |b| ms - b.0 > 60_000.0) { blocks.push((ms, vec![])); }
            blocks.last_mut().unwrap().1.push(text);
        }
        let body = blocks.iter().map(|(ms, t)| format!("## {}\n\n{}", timestamp(*ms), t.join(" "))).collect::<Vec<_>>().join("\n\n");
        let title = if title.is_empty() { format!("YouTube {id}") } else { title };
        self.save_source(dir, &title, &[("source", json!(format!("https://www.youtube.com/watch?v={id}"))), ("channel", json!(author)), ("type", json!("youtube"))], &body)
    }

    // ---------- files
    /// Any supported file. `original` = an already-saved original (reprocessing).
    pub async fn add_file(&self, dir: &str, filename: &str, bytes: &[u8], source_url: Option<&str>, original: Option<&str>) -> Result<String> {
        let e = ext(filename).to_lowercase();
        let title = stem(filename).to_string();
        match e.as_str() {
            "pdf" => self.add_pdf(dir, filename, bytes, source_url, original).await,
            "epub" => self.add_epub(dir, filename, bytes),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => self.add_image(dir, filename, bytes).await,
            "txt" | "md" | "markdown" => {
                let text = String::from_utf8_lossy(bytes);
                let (_, body) = crate::links::split_frontmatter(&text);
                self.save_source(dir, &title, &[("source", json!(filename)), ("type", json!("text"))], body)
            }
            "html" | "htm" => {
                let html = String::from_utf8_lossy(bytes).to_string();
                let (t, content) = dom_smoothie::Readability::new(html.as_str(), None, None).and_then(|mut r| r.parse()).map(|a| (a.title.to_string(), a.content.to_string())).unwrap_or((String::new(), html));
                self.save_source(dir, if t.is_empty() { &title } else { &t }, &[("source", json!(filename)), ("type", json!("web"))], &to_markdown(&content))
            }
            _ => Err(ApiError::bad_request(format!("Unsupported file type: .{e}"))),
        }
    }

    async fn add_pdf(&self, dir: &str, filename: &str, bytes: &[u8], source_url: Option<&str>, original: Option<&str>) -> Result<String> {
        let v = self.vault()?;
        let title = stem(filename).to_string();
        let pdfium = self.pdfium()?;
        let doc = pdfium.load_pdf_from_byte_slice(bytes, None).map_err(|e| ApiError::bad_request(format!("Couldn’t open the PDF: {e}")))?;
        let total = doc.pages().len() as usize;
        let mut pages: Vec<Vec<pdfmd::Line>> = vec![];
        for page in doc.pages().iter() {
            let mut glyphs: Vec<pdfmd::Glyph> = vec![];
            if let Ok(text) = page.text() {
                for ch in text.chars().iter() {
                    let Some(c) = ch.unicode_char() else { continue };
                    let Ok(b) = ch.loose_bounds() else { continue };
                    let bold = ch.font_weight().map(|w| matches!(w, pdfium_render::prelude::PdfFontWeight::Custom(n) if n >= 600) || matches!(w, pdfium_render::prelude::PdfFontWeight::Weight700Bold | pdfium_render::prelude::PdfFontWeight::Weight800 | pdfium_render::prelude::PdfFontWeight::Weight900)).unwrap_or(false)
                        || ch.font_name().to_lowercase().contains("bold");
                    glyphs.push((c, b.left().value, b.top().value, ch.scaled_font_size().value, bold, (b.right().value - b.left().value).max(0.0)));
                }
            }
            pages.push(pdfmd::lines_of(&glyphs));
        }
        let mut body = pdfmd::to_markdown(&pages);
        let mut ocr_used = false;
        // A scanned PDF has (almost) no text layer — render each page and OCR it.
        if body.chars().filter(|c| !c.is_whitespace()).count() / total.max(1) < 50 {
            ocr_used = true;
            let id = format!("ocr-{}", crate::srs::now_ms() as u64);
            let label = format!("OCR: {filename}");
            let mut out = vec![];
            for (i, page) in doc.pages().iter().enumerate() {
                self.job(&id, &label, i, total, &format!("Page {} of {total}", i + 1));
                let img = page.render_with_config(&pdfium_render::prelude::PdfRenderConfig::new().set_target_width(2000))
                    .map_err(|e| ApiError::new(500, e.to_string()))?.as_image().map_err(|e| ApiError::new(500, e.to_string()))?.into_rgb8();
                out.push(format!("## Page {}\n\n{}", i + 1, self.ocr(&img).await?));
            }
            self.job_end(&id, &label);
            body = out.join("\n\n");
        }
        // Keep the original next to the source so it can be opened and re-converted later.
        let files = Files::new(&v.path);
        let note_rel = join(dir, &format!("{}.md", safe_title(&title)));
        // Re-uploading a file that's already a source replaces its saved original instead of adding a copy.
        let previous = files.read(&note_rel).ok().and_then(|(c, _)| crate::links::split_frontmatter(&c).0["original"].as_str().map(String::from)).filter(|o| files.exists(o));
        let (att_path, att_name) = match original.map(String::from).or(previous) {
            Some(o) => { if original.is_none() { std::fs::write(files.safe(&o)?, bytes)?; } let n = basename(&o).to_string(); (o, n) }
            None => files.save_attachment(&note_rel, &format!("{title}.pdf"), bytes)?,
        };
        self.save_source(dir, &title, &[
            ("source", json!(source_url.unwrap_or(filename))), ("type", json!("pdf")), ("pages", json!(total)), ("original", json!(att_path)), ("ocr", if ocr_used { json!(true) } else { Value::Null }),
        ], &format!("> Original: [[{att_name}]] · {total} pages\n\n{body}"))
    }

    fn add_epub(&self, dir: &str, filename: &str, bytes: &[u8]) -> Result<String> {
        let bad = |m: &str| ApiError::bad_request(format!("Not a valid EPUB ({m})"));
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| bad("not a zip"))?;
        let read = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| -> Option<String> {
            use std::io::Read; let mut s = String::new(); zip.by_name(name).ok()?.read_to_string(&mut s).ok()?; Some(s)
        };
        let container = read(&mut zip, "META-INF/container.xml").ok_or_else(|| bad("no container"))?;
        let opf_path = regex::Regex::new(r#"full-path="([^"]+)""#).unwrap().captures(&container).map(|c| c[1].to_string()).ok_or_else(|| bad("no package file"))?;
        let opf = read(&mut zip, &opf_path).ok_or_else(|| bad("missing package file"))?;
        let base = dirname(&opf_path).to_string();
        let item = regex::Regex::new(r#"<item\b[^>]*>"#).unwrap();
        let attr = |tag: &str, name: &str| regex::Regex::new(&format!(r#"\b{name}="([^"]*)""#)).unwrap().captures(tag).map(|c| c[1].to_string());
        let manifest: std::collections::HashMap<String, String> = item.find_iter(&opf).filter_map(|m| Some((attr(m.as_str(), "id")?, attr(m.as_str(), "href")?))).collect();
        let spine: Vec<String> = regex::Regex::new(r#"<itemref\b[^>]*idref="([^"]+)""#).unwrap().captures_iter(&opf).filter_map(|c| manifest.get(&c[1]).cloned()).collect();
        let meta = |tag: &str| regex::Regex::new(&format!(r"(?s)<dc:{tag}[^>]*>(.*?)</dc:{tag}>")).unwrap().captures(&opf).map(|c| decode_entities(c[1].trim()));
        let title = meta("title").unwrap_or_else(|| stem(filename).to_string());
        let heading = regex::Regex::new(r"(?is)<(h1|h2|h3|title)[^>]*>(.*?)</").unwrap();
        let tags = regex::Regex::new(r"<[^>]+>").unwrap();
        let mut chapters = vec![];
        for href in &spine {
            let path = join(&base, &percent_encoding::percent_decode_str(href).decode_utf8_lossy());
            let Some(html) = read(&mut zip, &path) else { continue };
            let h = heading.captures(&html).map(|c| decode_entities(tags.replace_all(&c[2], "").trim()));
            let body_html = regex::Regex::new(r"(?is)<body[^>]*>(.*)</body>").unwrap().captures(&html).map(|c| c[1].to_string()).unwrap_or(html.clone());
            let md = to_markdown(&body_html);
            let md = regex::Regex::new(r"^#{1,6}\s+.*\n+").unwrap().replace(md.trim(), "").to_string();
            if md.chars().count() < 40 { continue; }
            let md = regex::Regex::new(r"(?m)^(#{1,4})\s").unwrap().replace_all(&md, "#$1 ").to_string();
            chapters.push(format!("## {}\n\n{md}", h.filter(|h| !h.is_empty()).unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1))));
        }
        if chapters.is_empty() { return Err(ApiError::bad_request("No readable chapters found in this EPUB")); }
        self.save_source(dir, &title, &[("source", json!(filename)), ("author", json!(meta("creator"))), ("type", json!("epub"))], &chapters.join("\n\n"))
    }

    async fn add_image(&self, dir: &str, filename: &str, bytes: &[u8]) -> Result<String> {
        let v = self.vault()?;
        let id = format!("ocr-{}", crate::srs::now_ms() as u64);
        let label = format!("OCR: {filename}");
        self.job(&id, &label, 0, 1, "Reading text…");
        let img = image::load_from_memory(bytes).map_err(|e| ApiError::bad_request(format!("Couldn’t read the image: {e}")))?.into_rgb8();
        let text = self.ocr(&img).await;
        self.job_end(&id, &label);
        let text = text?;
        let title = stem(filename).to_string();
        let (_, att_name) = Files::new(&v.path).save_attachment(&join(dir, &format!("{}.md", safe_title(&title))), filename, bytes)?;
        self.save_source(dir, &title, &[("source", json!(filename)), ("type", json!("image")), ("ocr", json!(true))],
            &format!("![[{att_name}]]\n\n{}", if text.trim().is_empty() { "_No text found in this image._".to_string() } else { text }))
    }

    /// Re-run conversion for an existing source from its saved original.
    pub async fn reprocess(&self, rel: &str) -> Result<String> {
        let v = self.vault()?;
        let files = Files::new(&v.path);
        let (content, _) = files.read(rel)?;
        let (fm, _) = crate::links::split_frontmatter(&content);
        let original = fm["original"].as_str().ok_or_else(|| ApiError::bad_request("This source has no saved original to re-convert. Upload the file again."))?.to_string();
        let bytes = std::fs::read(files.safe(&original)?)?;
        files.trash(rel)?;
        // Keep the note's own name (not the stored original's file name).
        let name = format!("{}.{}", stem(basename(rel)), ext(&original));
        self.add_file(dirname(rel), &name, &bytes, None, Some(&original)).await
    }

    // ---------- native engines
    /// PDFium can only be bound once per process, so the engine is loaded on first use and kept.
    fn pdfium(&self) -> Result<&'static pdfium_render::prelude::Pdfium> {
        use pdfium_render::prelude::Pdfium;
        static ENGINE: std::sync::OnceLock<Option<Pdfium>> = std::sync::OnceLock::new();
        let data_root = self.data_root.clone();
        ENGINE.get_or_init(|| {
            let mut candidates: Vec<PathBuf> = vec![];
            if let Ok(p) = std::env::var("VAULTLM_PDFIUM") { candidates.push(PathBuf::from(p)); }
            if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
                candidates.push(dir.join("../Resources/pdfium")); candidates.push(dir.join("pdfium")); candidates.push(dir);
            }
            candidates.push(data_root.join("pdfium"));
            for c in &candidates {
                let lib = Pdfium::pdfium_platform_library_name_at_path(c);
                if lib.exists() {
                    match Pdfium::bind_to_library(&lib) { Ok(b) => return Some(Pdfium::new(b)), Err(e) => eprintln!("PDFium at {} failed to load: {e:?}", lib.display()) }
                }
            }
            Pdfium::bind_to_system_library().ok().map(Pdfium::new)
        }).as_ref().ok_or_else(|| ApiError::new(500, "The PDF engine (PDFium) isn’t installed with this build."))
    }

    async fn ocr(&self, img: &image::RgbImage) -> Result<String> {
        let dir = self.data_root.join("ocrs");
        let mut paths = vec![];
        for name in ["text-detection.rten", "text-recognition.rten"] {
            let p = dir.join(name);
            if !p.exists() {
                std::fs::create_dir_all(&dir)?;
                // One-time download of the OCR models (~12 MB); recognition runs fully on-device afterwards.
                let bytes = http().get(format!("https://ocrs-models.s3-accelerate.amazonaws.com/{name}")).send().await.map_err(net)?.bytes().await.map_err(net)?;
                crate::util::write_atomic(&p, &bytes)?;
            }
            paths.push(p);
        }
        let (det, rec) = (paths[0].clone(), paths[1].clone());
        let img = img.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let e = |m: String| ApiError::new(500, format!("OCR: {m}"));
            let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
                detection_model: Some(rten::Model::load_file(det).map_err(|x| e(x.to_string()))?),
                recognition_model: Some(rten::Model::load_file(rec).map_err(|x| e(x.to_string()))?),
                ..Default::default()
            }).map_err(|x| e(x.to_string()))?;
            let src = ocrs::ImageSource::from_bytes(img.as_raw(), img.dimensions()).map_err(|x| e(x.to_string()))?;
            let input = engine.prepare_input(src).map_err(|x| e(x.to_string()))?;
            Ok(engine.get_text(&input).map_err(|x| e(x.to_string()))?.trim().to_string())
        }).await.map_err(|x| ApiError::new(500, x.to_string()))?
    }
}

pub fn youtube_id(url: &str) -> Option<String> {
    regex::Regex::new(r"(?:youtube\.com/(?:watch\?(?:.*&)?v=|shorts/|embed/|live/)|youtu\.be/)([\w-]{11})").unwrap().captures(url).map(|c| c[1].to_string())
}

/// Shown when YouTube's unofficial API answers in an unexpected way: the import is experimental.
const YT_CHANGED: &str = "YouTube import is experimental and stopped working for this video. YouTube may have changed something; try again later, or paste the transcript into a note.";

/// Caption tracks via the InnerTube player API (Android client), as the earlier version did.
/// This is YouTube's internal API, not a public one, so it can break without notice.
async fn youtube_tracks(id: &str) -> Result<(Vec<Value>, String, String)> {
    const VERSION: &str = "20.10.38";
    let res = http().post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", format!("com.google.android.youtube/{VERSION} (Linux; U; Android 14)"))
        .json(&json!({ "context": { "client": { "clientName": "ANDROID", "clientVersion": VERSION } }, "videoId": id }))
        .send().await.map_err(net)?;
    if !res.status().is_success() { return Err(ApiError::new(502, YT_CHANGED)); }
    let v: Value = res.json().await.map_err(|_| ApiError::new(502, YT_CHANGED))?;
    let playable = v.pointer("/playabilityStatus/status").and_then(|s| s.as_str()).unwrap_or("OK");
    if playable != "OK" {
        let why = v.pointer("/playabilityStatus/reason").and_then(|s| s.as_str()).unwrap_or("");
        return Err(ApiError::bad_request(if why.is_empty() { YT_CHANGED.to_string() } else { format!("YouTube: {why}") }));
    }
    let tracks = v.pointer("/captions/playerCaptionsTracklistRenderer/captionTracks").and_then(|t| t.as_array()).cloned().unwrap_or_default();
    if tracks.is_empty() { return Err(ApiError::bad_request("No transcript available for this video (captions are off or unavailable)")); }
    Ok((tracks, v.pointer("/videoDetails/title").and_then(|t| t.as_str()).unwrap_or("").into(), v.pointer("/videoDetails/author").and_then(|t| t.as_str()).unwrap_or("").into()))
}

/// srv3 (<p t="ms">) or classic (<text start="s">) caption XML → (start_ms, text).
pub fn parse_transcript(xml: &str) -> Vec<(f64, String)> {
    let p = regex::Regex::new(r#"(?s)<p\s+t="(\d+)"\s+d="(\d+)"[^>]*>(.*?)</p>"#).unwrap();
    let s = regex::Regex::new(r"<s[^>]*>([^<]*)</s>").unwrap();
    let tags = regex::Regex::new(r"<[^>]+>").unwrap();
    let clean = |t: &str| decode_entities(t).split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: Vec<(f64, String)> = p.captures_iter(xml).filter_map(|c| {
        let inner = &c[3];
        let mut text: String = s.captures_iter(inner).map(|m| m[1].to_string()).collect();
        if text.is_empty() { text = tags.replace_all(inner, "").to_string(); }
        let t = clean(&text);
        (!t.is_empty()).then(|| (c[1].parse::<f64>().unwrap_or(0.0), t))
    }).collect();
    if out.is_empty() {
        let classic = regex::Regex::new(r#"<text start="([^"]*)" dur="([^"]*)">([^<]*)</text>"#).unwrap();
        out = classic.captures_iter(xml).map(|c| (c[1].parse::<f64>().unwrap_or(0.0) * 1000.0, clean(&decode_entities(&c[3])))).filter(|(_, t)| !t.is_empty()).collect();
    }
    out
}

fn timestamp(ms: f64) -> String {
    let s = (ms / 1000.0) as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m:02}:{sec:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yt() {
        assert_eq!(youtube_id("https://youtu.be/arj7oStGLkU?t=3").as_deref(), Some("arj7oStGLkU"));
        assert_eq!(youtube_id("https://www.youtube.com/watch?feature=x&v=dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        let srv3 = r#"<timedtext><body><p t="12645" d="1370">So in college,</p><p t="75000" d="10"><s>and</s><s> then</s></p></body></timedtext>"#;
        assert_eq!(parse_transcript(srv3), vec![(12645.0, "So in college,".to_string()), (75000.0, "and then".to_string())]);
        assert_eq!(parse_transcript(r#"<text start="1.5" dur="2">I&#39;m here</text>"#), vec![(1500.0, "I'm here".to_string())]);
        assert_eq!(timestamp(12645.0), "00:12"); assert_eq!(timestamp(4_125_000.0), "1:08:45");
    }
}
