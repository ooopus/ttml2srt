use anyhow::{Context, Result};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::fmt::Write;

use crate::time::parse_time;
use crate::RubyMode;

/// The ruby semantic role of a TTML style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RubyRole {
    Container,
    Base,
    Text,
}

/// Build a map from style xml:id -> RubyRole by inspecting the <styling> section.
fn build_style_map(doc: &Document) -> HashMap<String, RubyRole> {
    let mut map = HashMap::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "style" {
            if let Some(id) = node
                .attribute(("http://www.w3.org/XML/1998/namespace", "id"))
                .or_else(|| node.attribute("id"))
            {
                // Look for tts:ruby attribute (in the tts namespace or as plain attribute)
                let ruby_val = node
                    .attribute(("http://www.w3.org/ns/ttml#styling", "ruby"))
                    .or_else(|| node.attribute("tts:ruby"));

                if let Some(rv) = ruby_val {
                    let role = match rv {
                        "container" => Some(RubyRole::Container),
                        "base" => Some(RubyRole::Base),
                        "text" | "textContainer" => Some(RubyRole::Text),
                        _ => None,
                    };
                    if let Some(role) = role {
                        map.insert(id.to_string(), role);
                    }
                }
            }
        }
    }
    map
}

/// Get the ruby role for a node based on its style attribute.
fn ruby_role_of(node: &Node, style_map: &HashMap<String, RubyRole>) -> Option<RubyRole> {
    node.attribute("style")
        .and_then(|s| style_map.get(s))
        .copied()
}

/// Extract visible text from a <p> element, handling ruby and <br/>.
fn extract_text(node: &Node, style_map: &HashMap<String, RubyRole>, ruby_mode: RubyMode) -> String {
    let mut out = String::new();
    extract_node(&mut out, node, style_map, ruby_mode, false);

    // Normalize whitespace around newlines
    let lines: Vec<&str> = out.split('\n').map(|l| l.trim()).collect();
    let result = lines.join("\n");
    result.trim().to_string()
}

/// Recursively extract text from a node.
///
/// If `in_ruby_container` is true, this node is inside a ruby container
/// and base/text collection is handled by the caller.
fn extract_node(
    out: &mut String,
    node: &Node,
    style_map: &HashMap<String, RubyRole>,
    ruby_mode: RubyMode,
    in_ruby_container: bool,
) {
    match node.node_type() {
        roxmltree::NodeType::Text => {
            out.push_str(node.text().unwrap_or(""));
        }
        roxmltree::NodeType::Element => {
            let tag = node.tag_name().name();

            // Handle <br/> / <br />
            if tag == "br" {
                out.push('\n');
                return;
            }

            // Handle HTML-style <ruby>
            if tag == "ruby" {
                extract_html_ruby(out, node, style_map, ruby_mode);
                return;
            }

            // Handle TTML-style ruby via style attributes
            let role = ruby_role_of(node, style_map);

            if role == Some(RubyRole::Container) && !in_ruby_container {
                extract_ttml_ruby_container(out, node, style_map, ruby_mode);
                return;
            }

            // For any other element, recurse into children
            for child in node.children() {
                extract_node(out, &child, style_map, ruby_mode, in_ruby_container);
            }
        }
        _ => {}
    }
}

/// Handle a TTML ruby container: collect base/text pairs from children.
fn extract_ttml_ruby_container(
    out: &mut String,
    container: &Node,
    style_map: &HashMap<String, RubyRole>,
    ruby_mode: RubyMode,
) {
    let mut bases: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();

    collect_ruby_parts(container, style_map, ruby_mode, &mut bases, &mut texts);

    emit_ruby_pairs(out, &bases, &texts, ruby_mode);
}

/// Recursively collect base and text content from inside a ruby container.
fn collect_ruby_parts(
    node: &Node,
    style_map: &HashMap<String, RubyRole>,
    ruby_mode: RubyMode,
    bases: &mut Vec<String>,
    texts: &mut Vec<String>,
) {
    for child in node.children() {
        match child.node_type() {
            roxmltree::NodeType::Element => {
                let role = ruby_role_of(&child, style_map);
                match role {
                    Some(RubyRole::Base) => {
                        let mut s = String::new();
                        extract_node(&mut s, &child, style_map, ruby_mode, true);
                        bases.push(s);
                    }
                    Some(RubyRole::Text) => {
                        let mut s = String::new();
                        extract_node(&mut s, &child, style_map, ruby_mode, true);
                        texts.push(s);
                    }
                    Some(RubyRole::Container) => {
                        // Nested container — recurse
                        collect_ruby_parts(&child, style_map, ruby_mode, bases, texts);
                    }
                    None => {
                        // Not a ruby-role span — recurse to find deeper roles
                        collect_ruby_parts(&child, style_map, ruby_mode, bases, texts);
                    }
                }
            }
            roxmltree::NodeType::Text => {
                // Bare text inside a ruby container (unusual) — treat as base
                let t = child.text().unwrap_or("");
                if !t.trim().is_empty() {
                    bases.push(t.to_string());
                }
            }
            _ => {}
        }
    }
}

/// Handle HTML-style <ruby> element: pair <rb>/<rt> children.
fn extract_html_ruby(
    out: &mut String,
    ruby_node: &Node,
    style_map: &HashMap<String, RubyRole>,
    ruby_mode: RubyMode,
) {
    let mut bases: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();

    for child in ruby_node.children() {
        if child.node_type() != roxmltree::NodeType::Element {
            continue;
        }
        let tag = child.tag_name().name();
        match tag {
            "rb" => {
                let mut s = String::new();
                extract_node(&mut s, &child, style_map, ruby_mode, true);
                bases.push(s);
            }
            "rt" => {
                let mut s = String::new();
                extract_node(&mut s, &child, style_map, ruby_mode, true);
                texts.push(s);
            }
            _ => {
                // Bare text between rb/rt pairs (some TTML uses this)
                let mut s = String::new();
                extract_node(&mut s, &child, style_map, ruby_mode, true);
                if !s.trim().is_empty() {
                    bases.push(s);
                }
            }
        }
    }

    emit_ruby_pairs(out, &bases, &texts, ruby_mode);
}

/// Emit matched base(text) pairs into the output string.
fn emit_ruby_pairs(out: &mut String, bases: &[String], texts: &[String], ruby_mode: RubyMode) {
    let max_pairs = bases.len().max(texts.len());
    for i in 0..max_pairs {
        let base = bases.get(i).map(|s| s.as_str()).unwrap_or("");
        let text = texts.get(i).map(|s| s.as_str()).unwrap_or("");

        match ruby_mode {
            RubyMode::Paren => {
                if !base.is_empty() {
                    out.push_str(base);
                }
                if !text.is_empty() {
                    out.push('(');
                    out.push_str(text);
                    out.push(')');
                }
            }
            RubyMode::Drop | RubyMode::Keepbase => {
                if !base.is_empty() {
                    out.push_str(base);
                }
            }
        }
    }
}

/// Convert a TTML XML string to an SRT string.
pub fn ttml_to_srt(xml: &str, ruby_mode: RubyMode) -> Result<String> {
    let doc = Document::parse(xml).context("failed to parse TTML XML")?;
    let style_map = build_style_map(&doc);

    let mut srt = String::new();
    let mut index = 0u32;

    for node in doc.descendants() {
        if node.tag_name().name() != "p" {
            continue;
        }

        let begin = match node.attribute("begin") {
            Some(b) => b,
            None => continue,
        };
        let end = match node.attribute("end") {
            Some(e) => e,
            None => continue,
        };

        let begin_time =
            parse_time(begin).with_context(|| format!("parsing begin time '{begin}'"))?;
        let end_time = parse_time(end).with_context(|| format!("parsing end time '{end}'"))?;

        let text = extract_text(&node, &style_map, ruby_mode);
        if text.is_empty() {
            continue;
        }

        index += 1;
        writeln!(srt, "{index}").unwrap();
        writeln!(srt, "{begin_time} --> {end_time}").unwrap();
        writeln!(srt, "{text}").unwrap();
        writeln!(srt).unwrap();
    }

    Ok(srt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(xml: &str) -> String {
        ttml_to_srt(xml, RubyMode::Paren).unwrap()
    }

    fn convert_drop(xml: &str) -> String {
        ttml_to_srt(xml, RubyMode::Drop).unwrap()
    }

    fn wrap_ttml(styling: &str, body_content: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<tt xmlns="http://www.w3.org/ns/ttml"
    xmlns:tts="http://www.w3.org/ns/ttml#styling"
    xmlns:ttp="http://www.w3.org/ns/ttml#parameter">
 <head>
  <styling>
   {styling}
  </styling>
 </head>
 <body>
  <div>
   {body_content}
  </div>
 </body>
</tt>"#
        )
    }

    #[test]
    fn test_simple_text() {
        let xml = wrap_ttml("", r#"<p begin="00:00:01.000" end="00:00:02.000">Hello world</p>"#);
        let srt = convert(&xml);
        assert!(srt.contains("Hello world"));
        assert!(srt.contains("00:00:01,000 --> 00:00:02,000"));
    }

    #[test]
    fn test_br_tag() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000">Line one<br/>Line two</p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.contains("Line one\nLine two"));
    }

    #[test]
    fn test_br_with_space() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000">Line one<br />Line two</p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.contains("Line one\nLine two"));
    }

    #[test]
    fn test_skip_no_begin() {
        let xml = wrap_ttml("", r#"<p end="00:00:02.000">Orphan</p>"#);
        let srt = convert(&xml);
        assert!(srt.is_empty());
    }

    #[test]
    fn test_skip_no_end() {
        let xml = wrap_ttml("", r#"<p begin="00:00:01.000">Orphan</p>"#);
        let srt = convert(&xml);
        assert!(srt.is_empty());
    }

    #[test]
    fn test_skip_empty_text() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000">   </p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.is_empty());
    }

    #[test]
    fn test_html_ruby() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000"><ruby><rb>慶應大学</rb><rt>けいおうだいがく</rt></ruby></p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.contains("慶應大学(けいおうだいがく)"));
    }

    #[test]
    fn test_html_ruby_multiple_pairs() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000"><ruby><rb>東</rb><rt>ひがし</rt><rb>京</rb><rt>きょう</rt></ruby></p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.contains("東(ひがし)京(きょう)"));
    }

    #[test]
    fn test_ttml_style_ruby() {
        let styling = r#"
            <style tts:ruby="container" xml:id="s5"></style>
            <style tts:ruby="base" xml:id="s6"></style>
            <style tts:ruby="text" xml:id="s7"></style>
        "#;
        let body = r#"<p begin="00:07:34.913" end="00:07:38.000"><span><span style="s5"><span style="s6">屍</span><span style="s7">しかばね</span></span>が10も超えたら<br/>誰も逆らわなくなるぞ</span></p>"#;
        let xml = wrap_ttml(styling, body);
        let srt = convert(&xml);
        assert!(srt.contains("屍(しかばね)が10も超えたら\n誰も逆らわなくなるぞ"));
    }

    #[test]
    fn test_ttml_style_ruby_drop_mode() {
        let styling = r#"
            <style tts:ruby="container" xml:id="s5"></style>
            <style tts:ruby="base" xml:id="s6"></style>
            <style tts:ruby="text" xml:id="s7"></style>
        "#;
        let body = r#"<p begin="00:07:34.913" end="00:07:38.000"><span style="s5"><span style="s6">屍</span><span style="s7">しかばね</span></span></p>"#;
        let xml = wrap_ttml(styling, body);
        let srt = convert_drop(&xml);
        assert!(srt.contains("屍"));
        assert!(!srt.contains("しかばね"));
    }

    #[test]
    fn test_ttml_style_ruby_senmetsu() {
        let styling = r#"
            <style tts:ruby="container" xml:id="s5"></style>
            <style tts:ruby="base" xml:id="s6"></style>
            <style tts:ruby="text" xml:id="s7"></style>
        "#;
        let body = r#"<p begin="00:11:51.420" end="00:11:54.923"><span>盗賊団を<span style="s5"><span style="s6">殲滅</span><span style="s7">せんめつ</span></span>し 村を守ったのです</span></p>"#;
        let xml = wrap_ttml(styling, body);
        let srt = convert(&xml);
        assert!(srt.contains("盗賊団を殲滅(せんめつ)し 村を守ったのです"));
    }

    #[test]
    fn test_nested_spans_no_ruby() {
        let xml = wrap_ttml(
            r#"<style tts:textAlign="center" xml:id="s1"></style>
               <style tts:textAlign="start" xml:id="s2"></style>"#,
            r#"<p begin="00:00:01.000" end="00:00:02.000" style="s1"><span style="s2">（ヴァン）無事に<br />フォレストドラゴン倒せてよかったね</span></p>"#,
        );
        let srt = convert(&xml);
        assert!(srt.contains("（ヴァン）無事に\nフォレストドラゴン倒せてよかったね"));
    }

    #[test]
    fn test_sequence_numbering() {
        let xml = wrap_ttml(
            "",
            r#"
            <p begin="00:00:01.000" end="00:00:02.000">First</p>
            <p begin="00:00:03.000" end="00:00:04.000">Second</p>
            <p begin="00:00:05.000" end="00:00:06.000">Third</p>
            "#,
        );
        let srt = convert(&xml);
        assert!(srt.starts_with("1\n"));
        assert!(srt.contains("\n2\n"));
        assert!(srt.contains("\n3\n"));
    }

    #[test]
    fn test_mismatched_ruby_base_extra() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000"><ruby><rb>漢</rb><rb>字</rb><rt>かん</rt></ruby></p>"#,
        );
        let srt = convert(&xml);
        // First base pairs with first text, second base has no text
        assert!(srt.contains("漢(かん)字"));
    }

    #[test]
    fn test_mismatched_ruby_text_extra() {
        let xml = wrap_ttml(
            "",
            r#"<p begin="00:00:01.000" end="00:00:02.000"><ruby><rb>漢</rb><rt>かん</rt><rt>じ</rt></ruby></p>"#,
        );
        let srt = convert(&xml);
        // First pair matches, second text has no base
        assert!(srt.contains("漢(かん)(じ)"));
    }
}
