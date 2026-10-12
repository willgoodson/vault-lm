//! SM-2 spaced repetition, persisted per vault in cards.json (same format as before).
use crate::util::{self, under, write_atomic};
use serde_json::{json, Map, Value};
use std::path::Path;

const DAY: f64 = 86_400_000.0;
pub fn now_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as f64 }

/// Same id as before: sha1(front \0 back)[..12].
pub fn card_id(front: &str, back: &str) -> String { sha1_hex(format!("{front}\0{back}").as_bytes())[..12].to_string() }

pub fn load(data: &Path) -> Map<String, Value> { util::read_json(&data.join("cards.json")).as_object().cloned().unwrap_or_default() }
fn save(data: &Path, db: &Map<String, Value>) -> std::io::Result<()> { write_atomic(&data.join("cards.json"), serde_json::to_string_pretty(db)?.as_bytes()) }

/// Cards in a deck include anything nested under it (notebook → sections → topics).
pub fn deck_cards(db: &Map<String, Value>, deck: &str) -> Vec<Value> {
    db.values().filter(|c| c["deck"].as_str().is_some_and(|d| under(d, deck))).cloned().collect()
}
pub fn deck(data: &Path, name: &str) -> Value {
    let cards = deck_cards(&load(data), name);
    let now = now_ms();
    let due = cards.iter().filter(|c| c["due"].as_f64().unwrap_or(0.0) <= now).count();
    json!({ "cards": cards, "due": due })
}
pub fn add_cards(data: &Path, deck: &str, cards: &[Value]) -> std::io::Result<()> {
    let mut db = load(data);
    for c in cards {
        let (front, back) = (c["front"].as_str().unwrap_or(""), c["back"].as_str().unwrap_or(""));
        let id = card_id(front, back);
        db.entry(id.clone()).or_insert_with(|| json!({ "front": front, "back": back, "cite": c["cite"].as_str().unwrap_or(""), "id": id, "deck": deck, "ease": 2.5, "interval": 0, "reps": 0, "due": now_ms() }));
    }
    save(data, &db)
}

/// grade: 0=again 1=hard 2=good 3=easy
pub fn review(data: &Path, id: &str, grade: u8) -> std::io::Result<Option<Value>> {
    let mut db = load(data);
    let Some(c) = db.get_mut(id) else { return Ok(None) };
    let now = now_ms();
    if grade == 0 { c["reps"] = json!(0); c["interval"] = json!(0); c["due"] = json!(now + 60_000.0); }
    else {
        let q = [0.0, 3.0, 4.0, 5.0][grade.min(3) as usize];
        let ease = (c["ease"].as_f64().unwrap_or(2.5) + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02)).max(1.3);
        let reps = c["reps"].as_u64().unwrap_or(0) + 1;
        let prev = c["interval"].as_f64().unwrap_or(0.0);
        let interval = match reps { 1 => if grade == 3 { 4.0 } else { 1.0 }, 2 => 6.0, _ => (prev * ease * match grade { 1 => 0.6, 3 => 1.3, _ => 1.0 }).round() };
        c["ease"] = json!(ease); c["reps"] = json!(reps); c["interval"] = json!(interval); c["due"] = json!(now + interval * DAY);
    }
    let out = c.clone();
    save(data, &db)?;
    Ok(Some(out))
}

// Minimal SHA-1 (card ids only; keeps ids identical to the previous version without another dependency).
fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 { w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]); }
        for i in 16..80 { w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1); }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i { 0..=19 => ((b & c) | (!b & d), 0x5A827999), 20..=39 => (b ^ c ^ d, 0x6ED9EBA1), 40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC), _ => (b ^ c ^ d, 0xCA62C1D6) };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d; d = c; c = b.rotate_left(30); b = a; a = t;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b); h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d); h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_match_node() { assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d"); assert_eq!(card_id("Q", "A").len(), 12); }
    #[test]
    fn sm2() {
        let d = tempfile::tempdir().unwrap();
        add_cards(d.path(), "P/S/T.md", &[json!({ "front": "Q", "back": "A" })]).unwrap();
        let id = card_id("Q", "A");
        assert_eq!(deck(d.path(), "P")["due"], 1); // notebook deck rolls up
        let c = review(d.path(), &id, 2).unwrap().unwrap();
        assert_eq!(c["interval"], 1.0); assert_eq!(c["reps"], 1);
        assert_eq!(deck(d.path(), "P")["due"], 0);
    }
}
