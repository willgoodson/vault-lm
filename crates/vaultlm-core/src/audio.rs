//! Audio overview: the model writes a two-host script, each line is voiced with the OS's built-in
//! speech engine (macOS `say`, Windows System.Speech, Linux espeak-ng), and the clips are joined into one WAV.
//! Using system voices keeps the app small and avoids shipping GPL phonemizer code.
use crate::error::{ApiError, Result};
use crate::files::Files;
use crate::layout::Scope;
use crate::util::{basename, join_rel};
use crate::Core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

impl Core {
    pub fn audio_status(&self, key: &str) -> Value { self.jobs.lock().get(key).cloned().unwrap_or_else(|| json!({ "status": "idle" })) }

    pub fn start_audio(&self, scope: Scope) -> Result<Value> {
        let key = format!("{}|{}", self.vault()?.path.display(), scope.topic.clone().unwrap_or_else(|| scope.base()));
        let (Some(core), Some(rt)) = (self.me.upgrade(), self.rt.clone()) else { return Err(ApiError::new(500, "No runtime")) };
        self.jobs.lock().insert(key.clone(), json!({ "status": "Writing script…" }));
        let k = key.clone();
        rt.spawn(async move {
            let set = |v: Value| { core.jobs.lock().insert(k.clone(), v); };
            match core.audio_overview(&scope, &|s: &str| { core.jobs.lock().insert(k.clone(), json!({ "status": s })); }).await {
                Ok(r) => set(json!({ "status": "done", "path": r["path"], "script": r["script"], "lines": r["lines"] })),
                Err(e) => set(json!({ "status": "error", "error": e.message })),
            }
        });
        Ok(json!({ "status": "Writing script…" }))
    }

    async fn audio_overview(&self, s: &Scope, progress: &(dyn Fn(&str) + Send + Sync)) -> Result<Value> {
        let v = self.vault()?;
        let text = crate::retrieve::scope_text(&*self.reader().await?, s, 16000).await?;
        if text.is_empty() { return Err(ApiError::bad_request("Nothing indexed in this scope yet")); }
        let raw = self.generate(&format!("Write a lively 3-5 minute podcast conversation between two hosts, A and B, discussing these notes. A explains, B asks curious questions and reacts. Each line must start with \"A:\" or \"B:\". No stage directions, no markdown.\n\nNotes:\n{text}")).await?;
        let re = regex::Regex::new(r"^\**\s*([AB])\s*\**:\s*(.+)").unwrap();
        let lines: Vec<(String, String)> = raw.lines().filter_map(|l| re.captures(l.trim()).map(|c| (c[1].to_string(), c[2].replace('*', "").trim().to_string()))).filter(|(_, t)| !t.is_empty()).collect();
        if lines.is_empty() { return Err(ApiError::new(502, "Model returned no script lines")); }
        let voices = pick_voices();
        let tmp = std::env::temp_dir().join(format!("vaultlm-audio-{}", crate::srs::now_ms() as u64));
        std::fs::create_dir_all(&tmp)?;
        let mut clips = vec![];
        for (i, (speaker, line)) in lines.iter().enumerate() {
            progress(&format!("Voicing line {}/{}", i + 1, lines.len()));
            let out = tmp.join(format!("{i:03}.wav"));
            let voice = if speaker == "A" { &voices.0 } else { &voices.1 };
            let text = line.clone(); let o = out.clone(); let vc = voice.clone();
            tokio::task::spawn_blocking(move || speak_to_wav(&text, vc.as_deref(), &o)).await.map_err(|e| ApiError::new(500, e.to_string()))??;
            clips.push(out);
        }
        let wav = join_wavs(&clips, 0.35)?;
        let _ = std::fs::remove_dir_all(&tmp);
        let script_path = Self::studio_path(s, "Audio Overview");
        let wav_rel = script_path.replace(".md", ".wav");
        let full = join_rel(&v.path, &wav_rel);
        if let Some(p) = full.parent() { std::fs::create_dir_all(p)?; }
        std::fs::write(&full, wav)?;
        let script = lines.iter().map(|(sp, t)| format!("**{sp}:** {t}")).collect::<Vec<_>>().join("\n\n");
        Files::new(&v.path).write(&script_path, &format!("![[{}]]\n\n{script}\n", basename(&wav_rel)))?;
        Ok(json!({ "path": wav_rel, "script": script_path, "lines": lines.len() }))
    }
}

/// Two distinct English voices if the OS has them.
fn pick_voices() -> (Option<String>, Option<String>) {
    if cfg!(target_os = "macos") {
        let out = Command::new("say").args(["-v", "?"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let have: Vec<&str> = out.lines().filter(|l| l.contains("en_") || l.contains("en-")).filter_map(|l| l.split_whitespace().next()).collect();
        let pref = ["Samantha", "Daniel", "Karen", "Moira", "Alex", "Tom", "Fred"];
        let mut chosen: Vec<String> = pref.iter().filter(|p| have.contains(p)).map(|s| s.to_string()).collect();
        for h in &have { if chosen.len() >= 2 { break; } if !chosen.iter().any(|c| c == h) { chosen.push(h.to_string()); } }
        return (chosen.first().cloned(), chosen.get(1).cloned().or_else(|| chosen.first().cloned()));
    }
    if cfg!(target_os = "linux") { return (Some("en-us".into()), Some("en-gb".into())); }
    (Some("0".into()), Some("1".into())) // Windows: installed voice indexes
}

fn speak_to_wav(text: &str, voice: Option<&str>, out: &Path) -> Result<()> {
    let err = |m: &str| ApiError::new(500, format!("Speech engine: {m}"));
    let status = if cfg!(target_os = "macos") {
        let mut c = Command::new("say");
        if let Some(v) = voice { c.args(["-v", v]); }
        c.args(["-o"]).arg(out).args(["--file-format=WAVE", "--data-format=LEI16@22050", text]).status()
    } else if cfg!(target_os = "windows") {
        let script = format!(
            "Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $v=$s.GetInstalledVoices(); if($v.Count -gt {i}){{$s.SelectVoice($v[{i}].VoiceInfo.Name)}}; $f=New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(22050,[System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,[System.Speech.AudioFormat.AudioChannel]::Mono); $s.SetOutputToWaveFile('{}',$f); $s.Speak([Console]::In.ReadToEnd()); $s.Dispose()",
            out.display().to_string().replace('\'', "''"), i = voice.unwrap_or("0"));
        let mut child = Command::new("powershell").args(["-NoProfile", "-Command", &script]).stdin(std::process::Stdio::piped()).spawn().map_err(|e| err(&e.to_string()))?;
        use std::io::Write;
        child.stdin.take().unwrap().write_all(text.as_bytes()).map_err(|e| err(&e.to_string()))?;
        child.wait()
    } else {
        let bin = if Command::new("espeak-ng").arg("--version").output().is_ok() { "espeak-ng" } else { "espeak" };
        let mut c = Command::new(bin);
        if let Some(v) = voice { c.args(["-v", v]); }
        c.arg("-w").arg(out).arg(text).status()
    };
    match status {
        Ok(s) if s.success() && out.exists() => Ok(()),
        Ok(_) => Err(err("synthesis failed")),
        Err(_) if cfg!(target_os = "linux") => Err(err("install espeak-ng to create audio overviews (e.g. sudo apt install espeak-ng)")),
        Err(e) => Err(err(&e.to_string())),
    }
}

/// Concatenate PCM WAV files (same format) with `gap` seconds of silence between them.
pub fn join_wavs(paths: &[PathBuf], gap: f32) -> Result<Vec<u8>> {
    let mut fmt: Option<(u16, u32, u16)> = None; // channels, sample rate, bits
    let mut data: Vec<u8> = vec![];
    for (i, p) in paths.iter().enumerate() {
        let bytes = std::fs::read(p)?;
        let (f, pcm) = parse_wav(&bytes).ok_or_else(|| ApiError::new(500, "Unreadable audio clip"))?;
        if fmt.is_none() { fmt = Some(f); }
        if Some(f) != fmt { return Err(ApiError::new(500, "Audio clips have different formats")); }
        if i > 0 {
            let (ch, rate, bits) = f;
            data.extend(std::iter::repeat_n(0u8, (rate as f32 * gap) as usize * ch as usize * (bits as usize / 8)));
        }
        data.extend_from_slice(pcm);
    }
    let (ch, rate, bits) = fmt.ok_or_else(|| ApiError::new(500, "No audio"))?;
    let block = ch * bits / 8;
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend(b"RIFF"); out.extend(&(36 + data.len() as u32).to_le_bytes()); out.extend(b"WAVEfmt ");
    out.extend(&16u32.to_le_bytes()); out.extend(&1u16.to_le_bytes()); out.extend(&ch.to_le_bytes()); out.extend(&rate.to_le_bytes());
    out.extend(&(rate * block as u32).to_le_bytes()); out.extend(&block.to_le_bytes()); out.extend(&bits.to_le_bytes());
    out.extend(b"data"); out.extend(&(data.len() as u32).to_le_bytes()); out.extend(data);
    Ok(out)
}

fn parse_wav(b: &[u8]) -> Option<((u16, u32, u16), &[u8])> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" { return None; }
    let (mut i, mut fmt, mut data) = (12usize, None, None);
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        let body = b.get(i + 8..(i + 8 + size).min(b.len()))?;
        if id == b"fmt " && body.len() >= 16 { fmt = Some((u16::from_le_bytes([body[2], body[3]]), u32::from_le_bytes(body[4..8].try_into().ok()?), u16::from_le_bytes([body[14], body[15]]))); }
        if id == b"data" { data = Some(body); }
        i += 8 + size + (size & 1);
    }
    Some((fmt?, data?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_join() {
        let d = tempfile::tempdir().unwrap();
        let mk = |n: &str, samples: usize| { let p = d.path().join(n);
            let data = vec![1u8; samples * 2]; let mut out = vec![]; out.extend(b"RIFF"); out.extend(&(36 + data.len() as u32).to_le_bytes()); out.extend(b"WAVEfmt ");
            out.extend(&16u32.to_le_bytes()); out.extend(&1u16.to_le_bytes()); out.extend(&1u16.to_le_bytes()); out.extend(&100u32.to_le_bytes()); out.extend(&200u32.to_le_bytes()); out.extend(&2u16.to_le_bytes()); out.extend(&16u16.to_le_bytes());
            out.extend(b"data"); out.extend(&(data.len() as u32).to_le_bytes()); out.extend(data); std::fs::write(&p, out).unwrap(); p };
        let joined = join_wavs(&[mk("a.wav", 10), mk("b.wav", 10)], 0.5).unwrap();
        let (_, pcm) = parse_wav(&joined).unwrap();
        assert_eq!(pcm.len(), 20 + 20 + 50 * 2);
    }
}
