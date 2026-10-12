//! Layout-aware PDF → Markdown using PDFium. Rebuilds lines from positioned characters, then uses font
//! size and real font weight to recover headings, strips running headers/footers and page numbers, fixes
//! symbol-font bullets, re-flows hard-wrapped paragraphs and de-hyphenates. (Port of the earlier pdf.js
//! converter; PDFium exposes bold, which pdf.js did not.)
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Line { pub y: f32, pub x: f32, pub text: String, pub size: f32, pub bold: bool }

/// One page of positioned characters: (char, x, y_top, font_size, bold, width).
pub type Glyph = (char, f32, f32, f32, bool, f32);

/// Group glyphs into lines (top of page first) and words.
pub fn lines_of(glyphs: &[Glyph]) -> Vec<Line> {
    let mut rows: Vec<(f32, Vec<&Glyph>)> = vec![];
    for g in glyphs {
        if g.0 == '\r' || g.0 == '\n' { continue; }
        let tol = (g.3 * 0.45).max(2.0);
        match rows.iter_mut().find(|(y, _)| (y - g.2).abs() <= tol) { Some(r) => r.1.push(g), None => rows.push((g.2, vec![g])) }
    }
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap()); // PDF y grows upward: top of page first
    rows.into_iter().filter_map(|(y, mut gs)| {
        gs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut text = String::new();
        let mut prev_end: Option<f32> = None;
        for g in &gs {
            if let Some(pe) = prev_end { if g.1 - pe > g.3 * 0.18 && !text.ends_with(' ') && g.0 != ' ' { text.push(' '); } }
            text.push(g.0);
            prev_end = Some(g.1 + g.5);
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() { return None; }
        let visible: Vec<&&Glyph> = gs.iter().filter(|g| !g.0.is_whitespace()).collect();
        let bold = !visible.is_empty() && visible.iter().filter(|g| g.4).count() as f32 / visible.len() as f32 > 0.6;
        Some(Line { y, x: gs[0].1, text, size: gs.iter().map(|g| g.3).fold(0.0, f32::max), bold })
    }).collect()
}

fn is_bullet_start(s: &str) -> bool {
    s.chars().next().is_some_and(|c| matches!(c, '\u{0}' | '•' | '‣' | '◦' | '⁃' | '∙' | '▪' | '▫' | '●' | '○' | '■' | '□' | '–' | '—' | '-' | '*' | '·') || ('\u{E000}'..='\u{F8FF}').contains(&c))
}
fn strip_bullet(s: &str) -> String { s.chars().skip(1).collect::<String>().trim_start().to_string() }
fn norm(s: &str) -> String { s.chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }

enum Block { H(usize, String, usize), P(String, usize, f32), Li(String, usize, f32) }

pub fn to_markdown(pages: &[Vec<Line>]) -> String {
    let total = pages.len();
    // Body size = the size carrying the most characters.
    let mut weight: HashMap<i32, usize> = HashMap::new();
    for l in pages.iter().flatten() { *weight.entry((l.size * 2.0).round() as i32).or_default() += l.text.len(); }
    let body = weight.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k as f32 / 2.0).unwrap_or(10.0);
    // Running headers/footers: same text (digits ignored) at the same position from top/bottom on many pages.
    let edge_key = |p: &[Line], i: usize| if i < 3 { Some(format!("t{i}|")) } else if i + 3 >= p.len() { Some(format!("b{}|", p.len() - 1 - i)) } else { None };
    let mut freq: HashMap<String, usize> = HashMap::new();
    for p in pages {
        let seen: HashSet<String> = p.iter().enumerate().filter_map(|(i, l)| edge_key(p, i).map(|e| e + &norm(&l.text))).collect();
        for k in seen { *freq.entry(k).or_default() += 1; }
    }
    let min_repeat = 3usize.max((total as f32 * 0.4).ceil() as usize);
    let running: HashSet<String> = freq.into_iter().filter(|(_, n)| *n >= min_repeat).map(|(k, _)| k).collect();
    let page_no = regex::Regex::new(r"(?i)^(page\s*)?\d{1,4}(\s*(of|/)\s*\d+)?$").unwrap();
    let toc = regex::Regex::new(r"(\.\s?){6,}\s*\d+$").unwrap();
    let labeled = regex::Regex::new(r"(?i)^(chapter|part|section|domain|module|unit|appendix|lesson)\s+[\dIVXLC]+\b").unwrap();
    let sublabel = regex::Regex::new(r"(?i)^(task|step|objective|topic|lab|exercise|question|activity|outcome)\s+\d+(\.\d+)*\b").unwrap();

    let mut blocks: Vec<Block> = vec![];
    for (pi, p) in pages.iter().enumerate() {
        let kept: Vec<&Line> = p.iter().enumerate().filter(|(i, l)| {
            !edge_key(p, *i).is_some_and(|e| running.contains(&(e + &norm(&l.text)))) && !page_no.is_match(&l.text) && !toc.is_match(&l.text)
        }).map(|(_, l)| l).collect();
        let mut prev: Option<&Line> = None;
        for l in kept {
            let short = l.text.chars().count() < 110 && !l.text.ends_with(['.', ';', ',']);
            let (big, medium) = (l.size >= body * 1.3, l.size >= body * 1.1);
            let is_label = labeled.is_match(&l.text);
            let is_sub = sublabel.is_match(&l.text) && l.text.chars().count() < 160;
            let level = if short && (big || (is_label && (medium || l.bold))) { 2 }
                else if short && (medium || (l.bold && l.text.chars().count() < 90 && !is_bullet_start(&l.text))) { 3 }
                else if short && is_label { 2 } else if is_sub { 3 } else { 0 };
            let gap = prev.map(|p| p.y - l.y).unwrap_or(0.0);
            let same_flow = prev.is_some_and(|p| gap > 0.0 && gap < l.size * 1.9 && (p.size - l.size).abs() < 0.6);
            let lower_start = l.text.chars().next().is_some_and(|c| c.is_lowercase() || c == '(');
            match (level, blocks.last_mut()) {
                (lv, Some(Block::H(_, t, bp))) if lv > 0 && *bp == pi && same_flow && prev.is_some_and(|p| p.bold == l.bold) && !is_sub => { t.push(' '); t.push_str(&l.text); }
                (lv, _) if lv > 0 => blocks.push(Block::H(lv, l.text.clone(), pi)),
                (_, Some(Block::H(3, t, bp))) if *bp == pi && same_flow && !is_bullet_start(&l.text) && lower_start => { *t = join_wrapped(t, &l.text); }
                _ if is_bullet_start(&l.text) => blocks.push(Block::Li(strip_bullet(&l.text), pi, l.x)),
                (_, Some(Block::P(t, _, x))) if same_flow && (l.x - *x).abs() < body * 3.0 => { *t = join_wrapped(t, &l.text); }
                (_, Some(Block::Li(t, _, x))) if same_flow && (l.x > *x + body * 0.25 || (lower_start && !t.ends_with(['.', '!', '?']))) => { *t = join_wrapped(t, &l.text); }
                (_, Some(Block::P(t, bp, _) | Block::Li(t, bp, _))) if *bp != pi && !t.ends_with(['.', '!', '?', ':']) && lower_start => { *t = join_wrapped(t, &l.text); }
                _ => blocks.push(Block::P(l.text.clone(), pi, l.x)),
            }
            prev = Some(l);
        }
    }
    let mut md = String::new();
    for b in blocks {
        match b {
            Block::H(lv, t, _) => md += &format!("\n\n{} {}\n\n", "#".repeat(lv), t),
            Block::Li(t, _, _) => md += &format!("- {t}\n"),
            Block::P(t, _, _) => md += &format!("\n{t}\n\n"),
        }
    }
    let cleaned: String = md.chars().filter(|c| !(c.is_control() && *c != '\n') && !('\u{E000}'..='\u{F8FF}').contains(c)).collect();
    regex::Regex::new(r"\n{3,}").unwrap().replace_all(cleaned.trim(), "\n\n").to_string()
}

fn join_wrapped(a: &str, b: &str) -> String {
    let mut ac = a.chars().rev();
    if ac.next() == Some('-') && ac.next().is_some_and(|c| c.is_alphabetic()) && b.chars().next().is_some_and(|c| c.is_lowercase()) { return format!("{}{b}", &a[..a.len() - 1]); }
    format!("{a} {b}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(y: f32, text: &str, size: f32, bold: bool) -> Line { Line { y, x: 45.0, text: text.into(), size, bold } }
    #[test]
    fn structure() {
        let header = |n: usize| vec![line(780.0, "©2023 Project Management Institute, Inc. All rights reserved", 8.0, false), line(770.0, "PMI Exam Content Outline", 8.0, false), line(760.0, &n.to_string(), 8.0, false)];
        let mut pages = vec![];
        for n in 1..=4 {
            let mut p = header(n);
            p.push(line(700.0, &format!("Domain {n} Fundamentals – 36%"), 16.0, true));
            p.push(line(680.0, "Task 1 Demonstrate an understanding of the various project life cycles and", 11.0, true));
            p.push(line(667.0, "processes.", 11.0, true));
            let w = ["alpha", "bravo", "charlie", "delta"][n - 1];
            p.push(line(650.0, &format!("\u{F0B7} Distinguish between a project, program, and a portfolio ({w})."), 11.0, false));
            p.push(line(620.0, &format!("Body text about {w} that wraps across a line and keeps manage-"), 11.0, false));
            p.push(line(607.0, &format!("ment together on page {n} of the {w} section."), 11.0, false));
            p.push(line(40.0, "Confidential draft", 8.0, false)); // a running footer
            pages.push(p);
        }
        let md = to_markdown(&pages);
        assert!(!md.contains("Project Management Institute"), "running header removed");
        assert!(md.contains("## Domain 1 Fundamentals – 36%"));
        assert!(md.contains("### Task 1 Demonstrate an understanding of the various project life cycles and processes."));
        assert!(md.contains("- Distinguish between a project, program, and a portfolio (alpha)."));
        assert!(md.contains("Body text about bravo that wraps across a line and keeps management together on page 2 of the bravo section."));
        assert!(!md.contains("Confidential draft"), "running footer removed");
    }
}
