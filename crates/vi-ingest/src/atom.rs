//! Minimal Atom reader for court opinion feeds.
//!
//! Only the fields ingestion actually stores are read. Entry summaries arrive
//! as escaped HTML inside XML, so the text is unescaped once by the XML reader
//! and then stripped of markup — never with a regex over the raw feed, which
//! is how mangled provenance gets into a database.
#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use quick_xml::XmlVersion;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    pub link: String,
    pub published: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    /// Escaped-HTML summary, already XML-unescaped.
    pub summary: String,
}

impl Entry {
    /// The summary as plain text: markup removed, whitespace collapsed, and
    /// the feed's own "Original document" link dropped.
    pub fn summary_text(&self) -> String {
        let text = strip_html(&self.summary);
        text.replace("Original document", "").trim().to_string()
    }
}

/// Parse the entries of an Atom feed.
pub fn parse(xml: &str) -> Result<Vec<Entry>> {
    let mut reader = Reader::from_str(xml);
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Option<Entry> = None;
    // Element path inside the current entry, so <author><name> is not confused
    // with the feed-level <name>.
    let mut path: Vec<String> = Vec::new();

    loop {
        match reader.read_event().context("reading the Atom feed")? {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                if name == "entry" {
                    current = Some(Entry::default());
                    path.clear();
                    continue;
                }
                if current.is_some() {
                    path.push(name);
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().as_ref());
                let Some(entry) = current.as_mut() else {
                    continue;
                };
                match name.as_str() {
                    "link" => {
                        let mut href = None;
                        let mut rel = None;
                        for attr in e.attributes() {
                            let attr = attr.context("reading a link attribute")?;
                            let value = attr
                                .normalized_value(XmlVersion::Implicit1_0)
                                .context("normalizing a link attribute")?
                                .to_string();
                            match local_name(attr.key.as_ref()).as_str() {
                                "href" => href = Some(value),
                                "rel" => rel = Some(value),
                                _ => {}
                            }
                        }
                        // `alternate` is the human-readable record; `enclosure`
                        // is a PDF and is not what we cite.
                        let is_alternate = rel.as_deref().unwrap_or("alternate") == "alternate";
                        if is_alternate && entry.link.is_empty() {
                            if let Some(href) = href {
                                entry.link = href;
                            }
                        }
                    }
                    "category" => {
                        for attr in e.attributes() {
                            let attr = attr.context("reading a category attribute")?;
                            if local_name(attr.key.as_ref()) == "term" {
                                entry.category = Some(
                                    attr.normalized_value(XmlVersion::Implicit1_0)
                                        .context("normalizing a category term")?
                                        .to_string(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(entry) = current.as_mut() {
                    push_text(entry, &path, &t.xml10_content());
                }
            }
            // Entity and character references arrive as their own events.
            Event::GeneralRef(r) => {
                let Some(entry) = current.as_mut() else {
                    continue;
                };
                let resolved = match r.resolve_char_ref().context("resolving a character ref")? {
                    Some(c) => Some(c.to_string()),
                    None => named_entity(&r).map(str::to_string),
                };
                if let Some(text) = resolved {
                    push_text(entry, &path, &text);
                }
            }
            Event::CData(t) => {
                if let Some(entry) = current.as_mut() {
                    push_text(entry, &path, &t);
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref());
                if name == "entry" {
                    if let Some(mut entry) = current.take() {
                        entry.title = entry.title.trim().to_string();
                        if !entry.link.is_empty() {
                            entries.push(entry);
                        }
                    }
                    path.clear();
                    continue;
                }
                if current.is_some() && path.last() == Some(&name) {
                    path.pop();
                }
            }
            _ => {}
        }
    }

    Ok(entries)
}

/// Route a chunk of character data to the field named by the current path.
fn push_text(entry: &mut Entry, path: &[String], text: &str) {
    if text.is_empty() {
        return;
    }
    match path.join("/").as_str() {
        "title" => entry.title.push_str(text),
        "summary" | "content" => entry.summary.push_str(text),
        "published" | "updated" => {
            if entry.published.is_none() && !text.trim().is_empty() {
                entry.published = Some(text.trim().to_string());
            }
        }
        "author/name" => {
            if !text.trim().is_empty() {
                entry
                    .author
                    .get_or_insert_with(String::new)
                    .push_str(text.trim());
            }
        }
        "id" => {
            if entry.link.is_empty() && !text.trim().is_empty() {
                entry.link = text.trim().to_string();
            }
        }
        _ => {}
    }
}

fn named_entity(name: &str) -> Option<&'static str> {
    match name {
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "nbsp" => Some(" "),
        _ => None,
    }
}

fn local_name(raw: &str) -> String {
    match raw.split_once(':') {
        Some((_, local)) => local.to_ascii_lowercase(),
        None => raw.to_ascii_lowercase(),
    }
}

/// Remove HTML markup and decode the entities that appear in court summaries.
pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        rest = &rest[idx..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let replacement = match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" | "#39" => Some("'".to_string()),
            "nbsp" | "#160" => Some(" ".to_string()),
            other => other
                .strip_prefix('#')
                .and_then(|n| n.parse::<u32>().ok())
                .and_then(char::from_u32)
                .map(|c| c.to_string()),
        };
        match replacement {
            Some(r) => {
                out.push_str(&r);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xml:lang="en-us" xmlns="http://www.w3.org/2005/Atom">
  <title>CourtListener.com: All opinions for the Ninth Circuit</title>
  <author><name>Free Law Project</name></author>
  <entry>
    <title>Doe v. County of Example</title>
    <link href="https://www.courtlistener.com/opinion/10967451/doe-v-county/" rel="alternate"/>
    <published>2026-09-04T00:00:00-07:00</published>
    <author><name>Court of Appeals for the Ninth Circuit</name></author>
    <id>https://www.courtlistener.com/opinion/10967451/doe-v-county/</id>
    <summary type="html">&lt;p&gt;No. 24-7676 The panel held that the prosecutor withheld
      exculpatory evidence &amp;amp; body-worn camera footage.&lt;/p&gt;&lt;br&gt;
      &lt;a href="/opinion/10967451/doe-v-county/"&gt;Original document&lt;/a&gt;</summary>
    <link href="https://storage.courtlistener.com/pdf/x.pdf" length="0" rel="enclosure" type="application/pdf"/>
    <category term="Published"/>
  </entry>
</feed>"#;

    #[test]
    fn reads_one_entry() {
        let entries = parse(FEED).expect("parses");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.title, "Doe v. County of Example");
        assert_eq!(
            e.link,
            "https://www.courtlistener.com/opinion/10967451/doe-v-county/"
        );
        assert_eq!(e.category.as_deref(), Some("Published"));
        assert_eq!(
            e.author.as_deref(),
            Some("Court of Appeals for the Ninth Circuit")
        );
        assert!(e.published.as_deref().unwrap().starts_with("2026-09-04"));
    }

    #[test]
    fn the_enclosure_pdf_never_becomes_the_citation() {
        let entries = parse(FEED).expect("parses");
        assert!(!entries[0].link.contains("storage.courtlistener.com"));
    }

    #[test]
    fn summary_becomes_plain_text() {
        let entries = parse(FEED).expect("parses");
        let text = entries[0].summary_text();
        assert!(text.starts_with("No. 24-7676"));
        assert!(text.contains("body-worn camera footage"));
        assert!(text.contains("evidence & body-worn"), "{text}");
        assert!(!text.contains('<'), "{text}");
        assert!(!text.contains("Original document"), "{text}");
    }

    #[test]
    fn feed_level_fields_are_not_mistaken_for_entries() {
        let entries = parse(FEED).expect("parses");
        assert_ne!(entries[0].author.as_deref(), Some("Free Law Project"));
        assert_ne!(
            entries[0].title,
            "CourtListener.com: All opinions for the Ninth Circuit"
        );
    }

    #[test]
    fn empty_feed_is_not_an_error() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Empty</title></feed>"#;
        assert!(parse(xml).expect("parses").is_empty());
    }

    #[test]
    fn entities_decode() {
        assert_eq!(decode_entities("a &amp; b &#39;c&#39;"), "a & b 'c'");
        assert_eq!(decode_entities("plain & simple"), "plain & simple");
    }
}
