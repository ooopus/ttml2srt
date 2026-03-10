use anyhow::Result;
use std::fmt::Write;

use crate::time::{parse_time, SrtTime};

/// Convert a WebVTT string to an SRT string.
pub fn vtt_to_srt(input: &str) -> Result<String> {
    let mut srt = String::new();
    let mut index = 0u32;

    for cue in parse_cues(input) {
        let text = clean_vtt_text(&cue.text);
        if text.is_empty() {
            continue;
        }

        index += 1;
        writeln!(srt, "{index}").unwrap();
        writeln!(srt, "{} --> {}", cue.start, cue.end).unwrap();
        writeln!(srt, "{text}").unwrap();
        writeln!(srt).unwrap();
    }

    Ok(srt)
}

struct Cue {
    start: SrtTime,
    end: SrtTime,
    text: String,
}

/// Parse VTT cues from the input text.
fn parse_cues(input: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for a timestamp line: contains " --> "
        if let Some((start, end)) = try_parse_timestamp_line(lines[i]) {
            i += 1;
            // Collect text lines until empty line or end
            let mut text_lines = Vec::new();
            while i < lines.len() && !lines[i].trim().is_empty() {
                text_lines.push(lines[i]);
                i += 1;
            }
            if !text_lines.is_empty() {
                cues.push(Cue {
                    start,
                    end,
                    text: text_lines.join("\n"),
                });
            }
        } else {
            i += 1;
        }
    }

    cues
}

/// Try to parse a VTT timestamp line like:
/// `00:00:26.026 --> 00:00:27.902 position:50.00%,middle align:middle ...`
fn try_parse_timestamp_line(line: &str) -> Option<(SrtTime, SrtTime)> {
    let arrow_pos = line.find("-->")?;
    let before = line[..arrow_pos].trim();
    let after_arrow = line[arrow_pos + 3..].trim();

    // The end timestamp is the first space-delimited token after "-->"
    // (everything after is positioning metadata)
    let end_str = after_arrow.split_whitespace().next()?;

    let start = parse_time(before).ok()?;
    let end = parse_time(end_str).ok()?;
    Some((start, end))
}

/// Clean VTT text: strip tags, decode entities, remove directional marks.
fn clean_vtt_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for line in text.lines() {
        if !result.is_empty() {
            result.push('\n');
        }
        let cleaned = strip_vtt_tags(line);
        let cleaned = decode_entities(&cleaned);
        let cleaned = remove_directional_marks(&cleaned);
        result.push_str(cleaned.trim());
    }

    result.trim().to_string()
}

/// Strip VTT/HTML-style tags like <c.japanese>, </c.japanese>, <b>, </b>, etc.
fn strip_vtt_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;

    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }

    out
}

/// Decode common HTML entities found in VTT.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&lrm;", "")
        .replace("&rlm;", "")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

/// Remove Unicode directional marks (LRM, RLM, LRE, RLE, PDF, LRO, RLO, LRI, RLI, FSI, PDI).
fn remove_directional_marks(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c,
            '\u{200E}' | '\u{200F}' |  // LRM, RLM
            '\u{202A}' | '\u{202B}' | '\u{202C}' | '\u{202D}' | '\u{202E}' |  // LRE..RLO
            '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'   // LRI..PDI
        ))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(vtt: &str) -> String {
        vtt_to_srt(vtt).unwrap()
    }

    #[test]
    fn test_simple_cue() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nHello world\n";
        let srt = convert(vtt);
        assert!(srt.contains("00:00:01,000 --> 00:00:02,000"));
        assert!(srt.contains("Hello world"));
    }

    #[test]
    fn test_strip_positioning() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000 position:50.00%,middle align:middle size:80.00% line:84.67%\nText\n";
        let srt = convert(vtt);
        assert!(srt.contains("00:00:01,000 --> 00:00:02,000"));
        assert!(srt.contains("Text"));
    }

    #[test]
    fn test_strip_class_tags() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\n<c.japanese>Hello</c.japanese>\n";
        let srt = convert(vtt);
        assert!(srt.contains("Hello"));
        assert!(!srt.contains("<c"));
    }

    #[test]
    fn test_strip_lrm() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\n<c.japanese>&lrm;（ヴァン）テスト</c.japanese>\n";
        let srt = convert(vtt);
        assert!(srt.contains("（ヴァン）テスト"));
        assert!(!srt.contains("&lrm;"));
        assert!(!srt.contains('\u{200E}'));
    }

    #[test]
    fn test_multiline_cue() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\n<c.japanese>&lrm;Line one\n&lrm;Line two</c.japanese>\n";
        let srt = convert(vtt);
        assert!(srt.contains("Line one\nLine two"));
    }

    #[test]
    fn test_ruby_parenthesized_preserved() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\n<c.japanese>&lrm;月見(るなみ)&lrm;ヤチヨ</c.japanese>\n";
        let srt = convert(vtt);
        assert!(srt.contains("月見(るなみ)ヤチヨ"));
    }

    #[test]
    fn test_skip_notes() {
        let vtt = "WEBVTT\n\nNOTE This is a comment\n\n1\n00:00:01.000 --> 00:00:02.000\nText\n";
        let srt = convert(vtt);
        assert!(srt.contains("Text"));
        assert!(!srt.contains("NOTE"));
        assert!(!srt.contains("comment"));
    }

    #[test]
    fn test_sequential_numbering() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nFirst\n\n2\n00:00:03.000 --> 00:00:04.000\nSecond\n";
        let srt = convert(vtt);
        assert!(srt.starts_with("1\n"));
        assert!(srt.contains("\n2\n"));
    }
}
