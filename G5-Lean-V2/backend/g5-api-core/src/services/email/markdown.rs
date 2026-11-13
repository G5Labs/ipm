//! The Markdown subset for text G5 did not write (`g5.email.markdown`) and
//! its link policy (`g5.email.link-policy`).
//!
//! The parser only produces events; this module writes every tag itself, with
//! fixed inline styles, and escapes every author value. Constructs outside the
//! subset are shown as their escaped source text, and the bounds refuse the
//! whole body.

use std::ops::Range;

use pulldown_cmark::{Alignment, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use super::render::escape_html;

/// Lists and block quotes each nest at most this deep.
const MAX_NESTING: usize = 2;
const MAX_TABLE_COLUMNS: usize = 10;
const MAX_TABLE_ROWS: usize = 200;
/// Bound on the rendered HTML of one body.
const MAX_RENDERED_BYTES: usize = 256 * 1024;

const FONT_MONO: &str = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace";
const BORDER: &str = "1px solid #d1d9e0";

/// Where author links may lead: the deployment's public G5 host and the
/// `https` repository scopes the requesting kind supplies.
pub(super) struct LinkPolicy {
    g5: Option<Origin>,
    scopes: Vec<Scope>,
}

#[derive(PartialEq, Eq)]
struct Origin {
    host: String,
    port: Option<u16>,
}

struct Scope {
    origin: Origin,
    /// Lowercase path without a trailing `/`, never empty.
    path: String,
}

impl LinkPolicy {
    /// `public_base_url` names the G5 host; each scope is an `https` URL
    /// with a non-empty path, and anything else is ignored.
    pub(super) fn new(public_base_url: &str, scopes: &[String]) -> Self {
        let g5 = url::Url::parse(public_base_url)
            .ok()
            .filter(|url| url.scheme() == "https")
            .and_then(|url| origin(&url));
        let scopes = scopes
            .iter()
            .filter_map(|scope| {
                let url = url::Url::parse(scope).ok()?;
                let path = url.path().trim_end_matches('/').to_ascii_lowercase();
                (url.scheme() == "https"
                    && url.username().is_empty()
                    && url.password().is_none()
                    && !path.is_empty())
                .then_some(())?;
                Some(Scope {
                    origin: origin(&url)?,
                    path,
                })
            })
            .collect();
        Self { g5, scopes }
    }

    /// The normalized `href` for `target`, when a link to it may be
    /// clickable. The returned string is the URL that was checked.
    fn href(&self, target: &str) -> Option<String> {
        let url = url::Url::parse(target.trim()).ok()?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let target_origin = origin(&url)?;
        let path = url.path().to_ascii_lowercase();
        // An encoded dot or separator could be decoded into another path by
        // the server after this check.
        if ["%2e", "%2f", "%5c"]
            .iter()
            .any(|escape| path.contains(escape))
        {
            return None;
        }
        let allowed = self.g5.as_ref() == Some(&target_origin)
            || self.scopes.iter().any(|scope| {
                scope.origin == target_origin
                    && (path == scope.path
                        || path
                            .strip_prefix(&scope.path)
                            .is_some_and(|rest| rest.starts_with('/')))
            });
        allowed.then(|| url.to_string())
    }
}

fn origin(url: &url::Url) -> Option<Origin> {
    Some(Origin {
        host: url.host_str()?.to_ascii_lowercase(),
        port: url.port_or_known_default(),
    })
}

/// A body outside the bounds: the reason names the construct and its line.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Refused(pub(super) String);

/// Render `source` (already normalized) as HTML.
pub(super) fn render(source: &str, policy: &LinkPolicy) -> Result<String, Refused> {
    let parser = Parser::new_ext(source, Options::ENABLE_TABLES).into_offset_iter();
    // The parser consumes definitions while reading the document, so they
    // cannot be shown in place: refuse rather than drop text.
    if let Some(span) = parser
        .reference_definitions()
        .iter()
        .map(|(_, definition)| definition.span.start)
        .min()
    {
        return Err(Refused(format!(
            "reference-style link definitions are not supported; write links inline as \
             [text](url) (line {})",
            line_of(source, span)
        )));
    }
    let mut writer = Writer {
        source,
        policy,
        out: vec![String::new()],
        links: Vec::new(),
        pending_text: String::new(),
        skip_until: None,
        lists: 0,
        quotes: 0,
        table: None,
        in_code_block: false,
        html_block: false,
    };
    for (event, range) in parser {
        writer.event(event, range)?;
    }
    writer.flush_text();
    let html = writer.out.pop().unwrap_or_default();
    if html.len() > MAX_RENDERED_BYTES {
        return Err(Refused(format!(
            "the rendered message is larger than {} KiB; shorten it or use fewer code spans \
             and table cells",
            MAX_RENDERED_BYTES / 1024
        )));
    }
    Ok(html)
}

/// The longest run of whole top-level blocks from the start of `source`
/// (already normalized) within `max_lines` lines and `max_chars` characters,
/// so a shortened text never ends inside a code fence, list item or table. A
/// top-level list that does not fit whole may be cut between its items.
/// Returns the kept source and whether anything was dropped, or `None` when
/// not even the first block or list item fits.
pub(super) fn leading_blocks(
    source: &str,
    max_lines: usize,
    max_chars: usize,
) -> Option<(String, bool)> {
    let fits = |end: usize| {
        let head = &source[..end];
        head.lines().count() <= max_lines && head.chars().count() <= max_chars
    };
    let mut depth = 0usize;
    let mut in_list = false;
    let mut kept_end = None;
    for (event, range) in Parser::new_ext(source, Options::ENABLE_TABLES).into_offset_iter() {
        // A block or item event's range spans the whole block or item.
        let (cut_point, list) = match &event {
            Event::End(_) => {
                depth -= 1;
                continue;
            }
            Event::Start(Tag::Item) => (depth == 1 && in_list, false),
            Event::Start(Tag::List(_)) => (depth == 0, depth == 0),
            _ => (depth == 0, false),
        };
        if depth == 0 {
            in_list = list;
        }
        if let Event::Start(_) = event {
            depth += 1;
        }
        if !cut_point {
            continue;
        }
        if fits(range.end) {
            kept_end = Some(range.end);
            // A whole list fits: its items need no separate cut.
            if list {
                in_list = false;
            }
        } else if !list {
            break;
        }
    }
    let end = kept_end?;
    let shortened = !source[end..].trim().is_empty();
    Some((source[..end].trim_end().to_string(), shortened))
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count() + 1
}

struct LinkFrame {
    target: String,
    /// The visible text, for the URL-like text rule.
    visible: String,
}

struct TableFrame {
    alignments: Vec<Alignment>,
    head: bool,
    cell: usize,
    rows: usize,
}

struct Writer<'a> {
    source: &'a str,
    policy: &'a LinkPolicy,
    /// Output buffers; a link's content renders into its own buffer until
    /// the link is decided.
    out: Vec<String>,
    links: Vec<LinkFrame>,
    /// Adjacent text events, joined so bare URLs are found whole.
    pending_text: String,
    /// While shown as source: the end tag that closes the construct.
    skip_until: Option<TagEnd>,
    lists: usize,
    quotes: usize,
    table: Option<TableFrame>,
    in_code_block: bool,
    html_block: bool,
}

impl Writer<'_> {
    fn push(&mut self, html: &str) {
        self.out
            .last_mut()
            .expect("an output buffer")
            .push_str(html);
    }

    fn visible(&mut self, text: &str) {
        if let Some(link) = self.links.last_mut() {
            link.visible.push_str(text);
        }
    }

    /// Escaped author text, shown as written.
    fn source_text(&mut self, range: Range<usize>) {
        let text = self.source[range]
            .trim_end_matches(['\n', '\r'])
            .to_string();
        self.visible(&text);
        self.push(&escape_html(&text));
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) -> Result<(), Refused> {
        if let Some(end) = self.skip_until {
            if matches!(&event, Event::End(tag) if *tag == end) {
                self.skip_until = None;
            }
            return Ok(());
        }
        if !matches!(event, Event::Text(_)) {
            self.flush_text();
        }
        match event {
            Event::Start(tag) => self.start(tag, range)?,
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.in_code_block || self.html_block {
                    self.push(&escape_html(&text));
                } else {
                    self.pending_text.push_str(&text);
                }
            }
            Event::Code(code) => {
                self.visible(&code);
                self.push(&format!(
                    "<code style=\"font-family:{FONT_MONO};font-size:90%;background:#f6f8fa;\
                     padding:1px 4px;border-radius:4px\">{}</code>",
                    escape_html(&code)
                ));
            }
            Event::Html(html) => self.push(&escape_html(&html)),
            Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_) => self.source_text(range),
            Event::SoftBreak | Event::HardBreak => {
                self.visible(" ");
                self.push("<br>");
            }
            Event::Rule => self.push(&format!(
                "<hr style=\"border:none;border-top:{BORDER};margin:16px 0\">"
            )),
        }
        Ok(())
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) -> Result<(), Refused> {
        let (source, start) = (self.source, range.start);
        let line = move || line_of(source, start);
        match tag {
            Tag::Paragraph => self.push("<p style=\"margin:0 0 12px\">"),
            Tag::Heading { level, .. } => match level {
                HeadingLevel::H1 => self.push("<h1 style=\"font-size:22px;margin:20px 0 8px\">"),
                HeadingLevel::H2 => self.push("<h2 style=\"font-size:18px;margin:20px 0 8px\">"),
                HeadingLevel::H3 => self.push("<h3 style=\"font-size:16px;margin:16px 0 8px\">"),
                _ => {
                    self.push("<p style=\"margin:0 0 12px\">");
                    self.source_text(range);
                    self.push("</p>");
                    self.skip_until = Some(TagEnd::Heading(level));
                }
            },
            Tag::BlockQuote(_) => {
                self.quotes += 1;
                if self.quotes > MAX_NESTING {
                    return Err(Refused(format!(
                        "block quotes nest at most {MAX_NESTING} levels (line {})",
                        line()
                    )));
                }
                self.push(
                    "<blockquote style=\"margin:0 0 12px;padding:0 12px;border-left:4px solid \
                     #d1d9e0;color:#59636e\">",
                );
            }
            // An info string selects no highlighting and is not shown.
            Tag::CodeBlock(_) => {
                self.in_code_block = true;
                self.push(&format!(
                    "<pre style=\"font-family:{FONT_MONO};font-size:90%;background:#f6f8fa;\
                     padding:12px;border-radius:6px;white-space:pre-wrap;margin:0 0 12px\">"
                ));
            }
            Tag::HtmlBlock => {
                self.html_block = true;
                self.push("<p style=\"margin:0 0 12px;white-space:pre-wrap\">");
            }
            Tag::List(start) => {
                self.lists += 1;
                if self.lists > MAX_NESTING {
                    return Err(Refused(format!(
                        "lists nest at most {MAX_NESTING} levels (line {})",
                        line()
                    )));
                }
                let style = "style=\"margin:0 0 12px;padding-left:24px\"";
                match start {
                    Some(1) => self.push(&format!("<ol {style}>")),
                    Some(n) => self.push(&format!("<ol start=\"{n}\" {style}>")),
                    None => self.push(&format!("<ul {style}>")),
                }
            }
            Tag::Item => self.push("<li>"),
            Tag::Table(alignments) => {
                if alignments.len() > MAX_TABLE_COLUMNS {
                    return Err(Refused(format!(
                        "tables have at most {MAX_TABLE_COLUMNS} columns (line {})",
                        line()
                    )));
                }
                self.table = Some(TableFrame {
                    alignments,
                    head: false,
                    cell: 0,
                    rows: 0,
                });
                self.push("<table style=\"border-collapse:collapse;margin:0 0 12px\">");
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.head = true;
                    table.cell = 0;
                }
                self.push("<thead><tr>");
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.cell = 0;
                    table.rows += 1;
                    if table.rows > MAX_TABLE_ROWS {
                        return Err(Refused(format!(
                            "tables have at most {MAX_TABLE_ROWS} rows (line {})",
                            line()
                        )));
                    }
                }
                self.push("<tr>");
            }
            Tag::TableCell => {
                let (cell, align) = match self.table.as_ref() {
                    Some(table) => (
                        if table.head { "th" } else { "td" },
                        match table.alignments.get(table.cell) {
                            Some(Alignment::Center) => "center",
                            Some(Alignment::Right) => "right",
                            _ => "left",
                        },
                    ),
                    None => ("td", "left"),
                };
                self.push(&format!(
                    "<{cell} style=\"border:{BORDER};padding:4px 8px;text-align:{align}\">"
                ));
            }
            Tag::Emphasis => self.push("<em>"),
            Tag::Strong => self.push("<strong>"),
            Tag::Link {
                link_type: LinkType::Inline | LinkType::Autolink | LinkType::Email,
                dest_url,
                ..
            } => {
                self.links.push(LinkFrame {
                    target: dest_url.to_string(),
                    visible: String::new(),
                });
                self.out.push(String::new());
            }
            Tag::Link { .. } => {
                self.source_text(range);
                self.skip_until = Some(TagEnd::Link);
            }
            Tag::Image { .. } => {
                self.source_text(range);
                self.skip_until = Some(TagEnd::Image);
            }
            // Only reachable with extensions this module leaves off; shown
            // as written all the same.
            Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::MetadataBlock(_) => {
                let end = tag.to_end();
                self.push("<span style=\"white-space:pre-wrap\">");
                self.source_text(range);
                self.push("</span>");
                self.skip_until = Some(end);
            }
        }
        Ok(())
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.push("</p>"),
            TagEnd::Heading(level) => self.push(match level {
                HeadingLevel::H1 => "</h1>",
                HeadingLevel::H2 => "</h2>",
                _ => "</h3>",
            }),
            TagEnd::BlockQuote(_) => {
                self.quotes -= 1;
                self.push("</blockquote>");
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.push("</pre>");
            }
            TagEnd::HtmlBlock => {
                self.html_block = false;
                self.push("</p>");
            }
            TagEnd::List(ordered) => {
                self.lists -= 1;
                self.push(if ordered { "</ol>" } else { "</ul>" });
            }
            TagEnd::Item => self.push("</li>"),
            TagEnd::Table => {
                self.table = None;
                self.push("</tbody></table>");
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.head = false;
                }
                self.push("</tr></thead><tbody>");
            }
            TagEnd::TableRow => self.push("</tr>"),
            TagEnd::TableCell => {
                let head = self.table.as_ref().is_some_and(|table| table.head);
                if let Some(table) = self.table.as_mut() {
                    table.cell += 1;
                }
                self.push(if head { "</th>" } else { "</td>" });
            }
            TagEnd::Emphasis => self.push("</em>"),
            TagEnd::Strong => self.push("</strong>"),
            TagEnd::Link => self.end_link(),
            _ => {}
        }
    }

    /// A link is clickable only under the link policy; otherwise its
    /// content is followed by its full target as text.
    fn end_link(&mut self) {
        let (Some(link), Some(content)) = (self.links.pop(), self.out.pop()) else {
            return;
        };
        let visible = link.visible.trim();
        let shows_target = visible == link.target.trim();
        let href = (shows_target || !url_like(visible))
            .then(|| self.policy.href(&link.target))
            .flatten();
        self.visible(&link.visible);
        match href {
            Some(href) => self.push(&format!(
                "<a href=\"{}\" style=\"color:#0969da\">{content}</a>",
                escape_html(&href)
            )),
            None if shows_target || link.target.trim().is_empty() => self.push(&content),
            None => {
                self.visible(&format!(" ({})", link.target));
                self.push(&format!("{content} ({})", escape_html(&link.target)));
            }
        }
    }

    /// Write joined text, making bare `https` URLs clickable when the
    /// policy allows them.
    fn flush_text(&mut self) {
        if self.pending_text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending_text);
        self.visible(&text);
        if !self.links.is_empty() {
            self.push(&escape_html(&text));
            return;
        }
        let mut html = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some((before, url, after)) = next_bare_url(rest) {
            html.push_str(&escape_html(before));
            match self.policy.href(url) {
                Some(href) => html.push_str(&format!(
                    "<a href=\"{}\" style=\"color:#0969da\">{}</a>",
                    escape_html(&href),
                    escape_html(url)
                )),
                None => html.push_str(&escape_html(url)),
            }
            rest = after;
        }
        html.push_str(&escape_html(rest));
        self.push(&html);
    }
}

/// The next bare `https://` URL that starts a word: (text before, URL, rest).
/// Trailing sentence punctuation and an unbalanced `)` stay outside the URL.
fn next_bare_url(text: &str) -> Option<(&str, &str, &str)> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("https://") {
        let start = from + found;
        let starts_word = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        if !starts_word {
            from = start + "https://".len();
            continue;
        }
        let len = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`'))
            .unwrap_or(text.len() - start);
        let mut url = &text[start..start + len];
        loop {
            let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'', '*', '_']);
            let trimmed = if trimmed.ends_with(')')
                && trimmed.matches(')').count() > trimmed.matches('(').count()
            {
                &trimmed[..trimmed.len() - 1]
            } else {
                trimmed
            };
            if trimmed.len() == url.len() {
                break;
            }
            url = trimmed;
        }
        if url.len() > "https://".len() {
            let end = start + url.len();
            return Some((&text[..start], url, &text[end..]));
        }
        from = start + "https://".len();
    }
    None
}

/// Visible text that reads as an address: a scheme separator, a `www.` word
/// or a `host.tld/` token. File names such as `render.rs` are not URL-like.
fn url_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("://") {
        return true;
    }
    lower
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '<' | '>'))
        .any(|word| {
            if word.starts_with("www.") {
                return true;
            }
            let Some((host, _)) = word.split_once('/') else {
                return false;
            };
            let Some((name, tld)) = host.rsplit_once('.') else {
                return false;
            };
            !name.is_empty()
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic())
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LinkPolicy {
        LinkPolicy::new(
            "https://g5.example/app/",
            &[
                "https://github.com/acme/api".to_string(),
                "https://git.acme.test/ghe/acme/web".to_string(),
                "http://github.com/acme/insecure".to_string(),
                "https://github.com/".to_string(),
            ],
        )
    }

    fn html(source: &str) -> String {
        render(source, &policy()).unwrap()
    }

    fn refused(source: &str) -> String {
        render(source, &policy()).unwrap_err().0
    }

    #[test]
    fn the_subset_renders_with_fixed_tags() {
        let out = html(
            "# One\n## Two\n### Three\n\nSome *em* and **strong** and `x<y`.\n\n\
             - a\n- b\n  1. c\n\n> quoted\n\n```rust\nfn main() {}\n```\n\n---\n",
        );
        for tag in [
            "<h1 ",
            "<h2 ",
            "<h3 ",
            "<em>em</em>",
            "<strong>strong</strong>",
            "<ul ",
            "<ol ",
            "<li>",
            "<blockquote ",
            "<pre ",
            "fn main() {}",
            "<hr ",
        ] {
            assert!(out.contains(tag), "{tag:?} in {out}");
        }
        assert!(out.contains(">x&lt;y</code>"));
        assert!(!out.contains("rust"), "the info string is not shown");
        let numbered = html("3. three\n4. four\n");
        assert!(numbered.contains("<ol start=\"3\" "));
    }

    #[test]
    fn line_breaks_inside_a_paragraph_are_kept() {
        assert!(html("Status: ok\nTests: pass").contains("Status: ok<br>Tests: pass"));
    }

    #[test]
    fn tables_render_with_alignment() {
        let out = html("| PR | State |\n|:--|--:|\n| #1 | open |\n");
        assert!(out.contains(
            "<th style=\"border:1px solid #d1d9e0;padding:4px 8px;text-align:left\">PR</th>"
        ));
        assert!(out.contains("text-align:right\">open</td>"));
    }

    #[test]
    fn constructs_outside_the_subset_show_as_escaped_source() {
        for (source, shown) in [
            (
                "<div onclick=\"x()\">hi</div>\n",
                "&lt;div onclick=&quot;x()&quot;&gt;hi&lt;/div&gt;",
            ),
            ("a <b>bold</b> c", "a &lt;b&gt;bold&lt;/b&gt; c"),
            (
                "<script>alert(1)</script>\n",
                "&lt;script&gt;alert(1)&lt;/script&gt;",
            ),
            ("<!-- note -->\n", "&lt;!-- note --&gt;"),
            (
                "![logo](https://evil.example/x.png)",
                "![logo](https://evil.example/x.png)",
            ),
            ("#### Deep heading", "#### Deep heading"),
            ("Note[^1].", "Note[^1]."),
            ("- [ ] todo", "[ ] todo"),
            ("~~gone~~", "~~gone~~"),
            ("&lt;b&gt;x&lt;/b&gt;", "&lt;b&gt;x&lt;/b&gt;"),
            ("<style>p{}</style>\n", "&lt;style&gt;p{}&lt;/style&gt;"),
            (
                "<iframe src=\"https://evil.example\"></iframe>\n",
                "&lt;iframe",
            ),
            (
                "<form><input></form>\n",
                "&lt;form&gt;&lt;input&gt;&lt;/form&gt;",
            ),
        ] {
            let out = html(source);
            assert!(out.contains(shown), "{source:?} → {out}");
            for tag in [
                "<div", "<b>", "<script", "<img", "<h4", "<iframe", "<form", "<style", "<del",
                "<input",
            ] {
                assert!(!out.contains(tag), "{source:?} produced {tag}: {out}");
            }
        }
    }

    #[test]
    fn reference_definitions_are_refused_with_their_line() {
        let reason = refused("Intro\n\nSee [docs][d].\n\n[d]: https://github.com/acme/api\n");
        assert!(
            reason.contains("reference-style link definitions"),
            "{reason}"
        );
        assert!(reason.contains("line 5"), "{reason}");
        // A footnote definition is not a link definition; it stays as text.
        assert!(html("Note.\n\n[^1]: footnote text\n").contains("[^1]: footnote text"));
    }

    #[test]
    fn bounds_refuse_with_the_construct_and_line() {
        assert!(html("- a\n  - b\n").contains("<li>b</li>"));
        let reason = refused("- a\n  - b\n    - c\n");
        assert!(
            reason.contains("lists nest at most 2") && reason.contains("line 3"),
            "{reason}"
        );
        let reason = refused("> a\n> > b\n> > > c\n");
        assert!(reason.contains("block quotes nest at most 2"), "{reason}");

        let wide = format!("|{}\n|{}\n", "c|".repeat(11), "-|".repeat(11));
        assert!(refused(&wide).contains("at most 10 columns"));
        let mut long = String::from("| a |\n|---|\n");
        long.push_str(&"| x |\n".repeat(200));
        assert!(html(&long).contains("<td"));
        long.push_str("| x |\n");
        let reason = refused(&long);
        assert!(
            reason.contains("at most 200 rows") && reason.contains("line 203"),
            "{reason}"
        );

        let codes = "`a` ".repeat(5_000);
        assert!(refused(&codes).contains("larger than 256 KiB"));
    }

    fn clickable(source: &str) -> Option<String> {
        let out = html(source);
        out.split("<a href=\"")
            .nth(1)
            .map(|rest| rest.split('"').next().unwrap().to_string())
    }

    #[test]
    fn links_to_g5_and_supplied_repositories_are_clickable() {
        for (source, href) in [
            (
                "[workspace](https://g5.example/acme/ws)",
                "https://g5.example/acme/ws",
            ),
            (
                "[PR 12](https://github.com/acme/api/pull/12)",
                "https://github.com/acme/api/pull/12",
            ),
            (
                "[repo](https://github.com/acme/api)",
                "https://github.com/acme/api",
            ),
            (
                "[repo](https://GitHub.com/ACME/Api/commit/abc)",
                "https://github.com/ACME/Api/commit/abc",
            ),
            (
                "[web](https://git.acme.test/ghe/acme/web/pull/3)",
                "https://git.acme.test/ghe/acme/web/pull/3",
            ),
            (
                "[render.rs](https://github.com/acme/api/blob/main/render.rs)",
                "https://github.com/acme/api/blob/main/render.rs",
            ),
            (
                "<https://github.com/acme/api/issues/9>",
                "https://github.com/acme/api/issues/9",
            ),
            (
                "[https://github.com/acme/api](https://github.com/acme/api)",
                "https://github.com/acme/api",
            ),
            (
                "PR: https://github.com/acme/api/pull/7.",
                "https://github.com/acme/api/pull/7",
            ),
            (
                "(see https://g5.example/acme/ws)",
                "https://g5.example/acme/ws",
            ),
        ] {
            assert_eq!(clickable(source).as_deref(), Some(href), "{source:?}");
        }
        let out = html("PR: https://github.com/acme/api/pull/7.");
        assert!(
            out.contains(">https://github.com/acme/api/pull/7</a>.</p>"),
            "{out}"
        );
    }

    #[test]
    fn other_links_show_their_target_and_are_not_clickable() {
        for (source, shown) in [
            (
                "[login](https://evil.example/login)",
                "login (https://evil.example/login)",
            ),
            (
                "[x](https://github.com/acme/apix)",
                "x (https://github.com/acme/apix)",
            ),
            (
                "[x](https://github.com/other/repo)",
                "x (https://github.com/other/repo)",
            ),
            (
                "[x](https://github.com/acme/api/../../evil/repo)",
                "x (https://github.com/acme/api/../../evil/repo)",
            ),
            (
                "[x](https://github.com/acme/api/%2e%2e/%2e%2e/evil)",
                "(https://github.com/acme/api/%2e%2e/%2e%2e/evil)",
            ),
            (
                "[x](http://github.com/acme/api)",
                "x (http://github.com/acme/api)",
            ),
            (
                "[x](http://github.com/acme/insecure/a)",
                "x (http://github.com/acme/insecure/a)",
            ),
            (
                "[x](https://user:pw@github.com/acme/api)",
                "x (https://user:pw@github.com/acme/api)",
            ),
            (
                "[x](https://g5.example:8443/acme)",
                "x (https://g5.example:8443/acme)",
            ),
            ("[mail](mailto:a@b.example)", "mail (mailto:a@b.example)"),
            ("[run](javascript:alert(1))", "run (javascript:alert(1))"),
            ("[d](data:text/html,hi)", "d (data:text/html,hi)"),
            ("[rel](/acme/ws)", "rel (/acme/ws)"),
            ("<a@b.example>", "a@b.example"),
            (
                "[https://g5.example/login](https://github.com/acme/api)",
                "https://g5.example/login (https://github.com/acme/api)",
            ),
            (
                "[www.g5labs.ai](https://github.com/acme/api)",
                "www.g5labs.ai (https://github.com/acme/api)",
            ),
            (
                "[g5labs.ai/login](https://github.com/acme/api)",
                "g5labs.ai/login (https://github.com/acme/api)",
            ),
            (
                "see https://evil.example/x now",
                "see https://evil.example/x now",
            ),
            (
                "see http://github.com/acme/api now",
                "see http://github.com/acme/api now",
            ),
            (
                "`https://github.com/acme/api`",
                "https://github.com/acme/api",
            ),
        ] {
            let out = html(source);
            assert!(!out.contains("<a "), "{source:?} → {out}");
            assert!(out.contains(&escape_html(shown)), "{source:?} → {out}");
        }
    }

    #[test]
    fn link_titles_and_markup_inside_links_are_handled() {
        let out = html("[**PR** 3](https://github.com/acme/api/pull/3 \"G5 security\")");
        assert!(out.contains("<a href=\"https://github.com/acme/api/pull/3\" style=\"color:#0969da\"><strong>PR</strong> 3</a>"), "{out}");
        assert!(!out.contains("security"), "titles are dropped");
        // The image source is URL-like visible text, so the link is shown.
        let out = html("[![img](https://github.com/acme/api/x.png)](https://github.com/acme/api)");
        assert!(
            out.contains("![img](https://github.com/acme/api/x.png) (https://github.com/acme/api)"),
            "{out}"
        );
        assert!(!out.contains("<img") && !out.contains("<a "));
    }

    #[test]
    fn a_policy_ignores_scopes_that_are_not_https_repositories() {
        let policy = LinkPolicy::new("http://localhost:5173", &["not a url".to_string()]);
        assert!(policy.scopes.is_empty());
        assert_eq!(policy.href("http://localhost:5173/x"), None);
        assert_eq!(policy.href("https://localhost:5173/x"), None);
        assert_eq!(policy.href("https://github.com/"), None);
    }

    #[test]
    fn url_like_text_is_addresses_not_file_names() {
        for text in [
            "https://x",
            "go to www.example.com",
            "evil.com/login",
            "Visit EVIL.COM/x",
        ] {
            assert!(url_like(text), "{text:?}");
        }
        for text in [
            "render.rs",
            "src/render.rs",
            "PR 12",
            "v1.2/notes",
            "e.g. this",
        ] {
            assert!(!url_like(text), "{text:?}");
        }
    }

    #[test]
    fn leading_blocks_keep_whole_top_level_blocks() {
        let source = "## Summary\n\nIntro.\n\n```sh\none\ntwo\nthree\n```\n\n- a\n- b\n";
        // The fence would cross the line budget, so the cut falls before it.
        assert_eq!(
            leading_blocks(source, 6, 1_500),
            Some(("## Summary\n\nIntro.".to_string(), true))
        );
        // A list, table or fence is kept whole or dropped whole.
        let (kept, shortened) = leading_blocks(source, 9, 1_500).unwrap();
        assert!(kept.ends_with("three\n```"), "{kept}");
        assert!(shortened);
        assert!(render(&kept, &policy()).unwrap().contains("<pre"));
        // Within both budgets everything is kept.
        assert_eq!(
            leading_blocks(source, 30, 1_500),
            Some((source.trim_end().to_string(), false))
        );
        // The character budget cuts at a block boundary too.
        let paragraphs = format!("{}\n\n{}", "a".repeat(800), "b".repeat(800));
        assert_eq!(
            leading_blocks(&paragraphs, 30, 1_500),
            Some(("a".repeat(800), true))
        );
        // A list that does not fit whole is cut between its items.
        let list = "Intro.\n\n- one\n- two\n  - nested\n- three\n\nAfter.";
        assert_eq!(
            leading_blocks(list, 5, 1_500),
            Some(("Intro.\n\n- one\n- two\n  - nested".to_string(), true))
        );
        assert_eq!(
            leading_blocks(list, 4, 1_500),
            Some(("Intro.\n\n- one".to_string(), true))
        );
        // When not even the first block fits, there is nothing to keep.
        assert_eq!(leading_blocks(&"word ".repeat(400), 30, 1_500), None);
        assert_eq!(leading_blocks("", 30, 1_500), None);
    }
}
