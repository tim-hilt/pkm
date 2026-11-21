//! Articles (Bookmarks ADR-0001): the readable text of a page as the Markdown
//! subset of ADR-0028. Readability-style scoring (text density, link density,
//! class and id hints) finds the main text; menus, ads, comment sections,
//! cookie banners and footers are left out; the page's `<head>` is `extract`'s.
//! Pure: the client downloads, this only reads. Text only for now.

use crate::address::resolve;
use crate::html::{Dom, Part};
use std::collections::BTreeMap;

/// The text of a page, and whether the page reads like an article.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Article {
    /// The main text; empty where the page has none. Kept even where the page
    /// reads like no article, for a user who wants it anyway.
    pub markdown: String,
    /// Whether to keep `markdown` unless the user says otherwise: the main
    /// text is long, and mostly text rather than links.
    pub reads_like_article: bool,
}

/// The Article of the page at `address` whose HTML is `html`. Links are made
/// absolute against `address`. Empty `html` gives no text.
pub fn read_article(html: &str, address: &str) -> Article {
    let dom = Dom::parse(html);
    let top = best_candidate(&dom).unwrap_or_else(|| body(&dom));
    let mut writer = Writer::new(&dom, address);
    writer.node(top);
    let blocks = writer.finish();
    let prose: usize = blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Paragraph)
        .map(|b| b.text.chars().count())
        .sum();
    let (text, linked) = text_length(&dom, top);
    Article {
        reads_like_article: prose >= MIN_PROSE && (linked as f64) < MAX_LINK_DENSITY * text as f64,
        markdown: render(&blocks),
    }
}

/// An Article has at least this many characters in paragraphs...
const MIN_PROSE: usize = 500;
/// ...and no more than this share of its text in links.
const MAX_LINK_DENSITY: f64 = 0.3;

fn body(dom: &Dom) -> usize {
    (0..dom.len())
        .find(|&id| dom.name(id) == "body")
        .unwrap_or(Dom::ROOT)
}

// Finding the main text

/// Elements that are no text of the page: chrome, media and forms' controls.
const LEFT_OUT: &[&str] = &[
    "aside", "audio", "button", "canvas", "dialog", "embed", "footer", "head", "iframe", "input",
    "link", "meta", "nav", "noscript", "object", "script", "select", "style", "svg", "template",
    "textarea", "video",
];

/// Class and id words that mark chrome wherever they are.
const CHROME: &[&str] = &[
    "ad",
    "ads",
    "advert",
    "banner",
    "comment",
    "consent",
    "cookie",
    "disqus",
    "gdpr",
    "modal",
    "newsletter",
    "popup",
    "promo",
    "sponsor",
    "subscribe",
];

/// Class and id words that mark chrome unless the element is also the article itself.
const MAYBE_CHROME: &[&str] = &[
    "breadcrumb",
    "footer",
    "masthead",
    "menu",
    "nav",
    "pagination",
    "recommended",
    "related",
    "share",
    "sharing",
    "sidebar",
    "social",
    "toolbar",
    "widget",
];

/// Class and id words that say an element is the article itself.
const ARTICLE: &[&str] = &["article", "body", "entry", "main"];

/// Class and id words that make an element more, and less, likely the main text.
const LIKELY: &[&str] = &[
    "article", "body", "content", "entry", "main", "page", "post", "story", "text",
];
const UNLIKELY: &[&str] = &[
    "foot", "meta", "shopping", "sidebar", "tag", "tool", "widget",
];

/// The words of an element's class and id, lower case.
fn words(dom: &Dom, id: usize) -> Vec<String> {
    let joined = format!(
        "{} {}",
        dom.attribute(id, "class").unwrap_or_default(),
        dom.attribute(id, "id").unwrap_or_default()
    );
    joined
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `word` is, or starts with, one of `list`, which for a short entry would catch
/// too much (`ad`, `address`).
fn is_any(word: &str, list: &[&str]) -> bool {
    list.iter()
        .any(|&entry| word == entry || (entry.len() > 3 && word.starts_with(entry)))
}

/// Whether the element is chrome, hidden, or never text: it and all in it are left out.
fn is_left_out(dom: &Dom, id: usize) -> bool {
    let name = dom.name(id);
    if LEFT_OUT.contains(&name) {
        return true;
    }
    let attribute = |key| dom.attribute(id, key);
    let hidden = attribute("hidden").is_some()
        || attribute("aria-hidden") == Some("true")
        || attribute("style").is_some_and(|s| s.replace(' ', "").contains("display:none"))
        || matches!(
            attribute("role"),
            Some("navigation" | "banner" | "complementary" | "contentinfo" | "dialog")
        );
    if hidden {
        return true;
    }
    if matches!(name, "html" | "body") {
        return false;
    }
    let words = words(dom, id);
    words.iter().any(|w| is_any(w, CHROME))
        || (words.iter().any(|w| is_any(w, MAYBE_CHROME))
            && !words.iter().any(|w| ARTICLE.contains(&w.as_str())))
}

/// The element that holds the main text: the one whose paragraphs, those of
/// its descendants and (half) those below its children score highest, less its link density.
fn best_candidate(dom: &Dom) -> Option<usize> {
    let mut paragraphs = Vec::new();
    collect_paragraphs(dom, Dom::ROOT, &mut paragraphs);
    let mut scores: BTreeMap<usize, f64> = BTreeMap::new();
    for p in paragraphs {
        let (text, _) = text_length(dom, p);
        if text < 25 {
            continue;
        }
        let commas = text_of(dom, p).matches(',').count();
        let score = 1.0 + commas as f64 + (text as f64 / 100.0).min(3.0);
        let parent = dom.parent(p);
        for (ancestor, share) in [(parent, 1.0), (dom.parent(parent), 0.5)] {
            if ancestor != Dom::ROOT || share == 1.0 {
                *scores
                    .entry(ancestor)
                    .or_insert_with(|| initial_score(dom, ancestor)) += score * share;
            }
        }
    }
    scores
        .into_iter()
        .map(|(id, score)| {
            let (text, linked) = text_length(dom, id);
            (id, score * (1.0 - linked as f64 / text.max(1) as f64))
        })
        .fold(None, |best: Option<(usize, f64)>, found| match best {
            Some(b) if b.1 >= found.1 => Some(b),
            _ => Some(found),
        })
        .map(|(id, _)| id)
}

/// The `<p>`, `<pre>` and `<td>` elements that are not left out.
fn collect_paragraphs(dom: &Dom, id: usize, found: &mut Vec<usize>) {
    for &child in dom.children(id) {
        if matches!(dom.part(child), Some(Part::Element(_))) && !is_left_out(dom, child) {
            if matches!(dom.name(child), "p" | "pre" | "td") {
                found.push(child);
            }
            collect_paragraphs(dom, child, found);
        }
    }
}

/// What an element's tag, class and id say before its paragraphs are counted.
fn initial_score(dom: &Dom, id: usize) -> f64 {
    let tag = match dom.name(id) {
        "article" => 10.0,
        "div" => 5.0,
        "pre" | "td" | "blockquote" => 3.0,
        "address" | "ol" | "ul" | "dl" | "dd" | "dt" | "li" | "form" => -3.0,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" => -5.0,
        _ => 0.0,
    };
    let words = words(dom, id);
    let hint = |list| words.iter().any(|w| is_any(w, list));
    tag + if hint(LIKELY) { 25.0 } else { 0.0 } - if hint(UNLIKELY) { 25.0 } else { 0.0 }
}

/// The characters of text under the node, and of those in links; left-out elements have none.
fn text_length(dom: &Dom, id: usize) -> (usize, usize) {
    fn sum(dom: &Dom, id: usize, in_link: bool) -> (usize, usize) {
        match dom.part(id) {
            Some(Part::Text(text)) => {
                let n = text
                    .split_whitespace()
                    .map(|w| w.chars().count() + 1)
                    .sum::<usize>();
                (n, if in_link { n } else { 0 })
            }
            Some(Part::Element(_)) if is_left_out(dom, id) => (0, 0),
            _ => {
                let in_link = in_link || dom.name(id) == "a";
                dom.children(id)
                    .iter()
                    .map(|&c| sum(dom, c, in_link))
                    .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
            }
        }
    }
    sum(dom, id, false)
}

fn text_of(dom: &Dom, id: usize) -> String {
    let mut out = String::new();
    fn push(dom: &Dom, id: usize, out: &mut String) {
        match dom.part(id) {
            Some(Part::Text(text)) => out.push_str(text),
            Some(Part::Element(_)) if is_left_out(dom, id) => {}
            _ => dom.children(id).iter().for_each(|&c| push(dom, c, out)),
        }
    }
    push(dom, id, &mut out);
    out
}

// Writing Markdown

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Paragraph,
    /// A list item, which follows another without a blank line.
    Item,
    /// A table row, likewise.
    Row,
    Other,
}

struct Block {
    kind: BlockKind,
    /// Follows the block before without a blank line.
    tight: bool,
    text: String,
}

fn render(blocks: &[Block]) -> String {
    let mut out = String::new();
    for (i, block) in blocks.iter().enumerate() {
        out += match (i, block.tight) {
            (0, _) => "",
            (_, true) => "\n",
            (_, false) => "\n\n",
        };
        out += &block.text;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// A span of inline Markdown (`**`, `[`..`](…)`) that is written once text is inside it.
struct Span {
    open: String,
    close: String,
    written: bool,
}

struct List {
    ordered: bool,
    count: u32,
}

/// Turns the nodes of a page into blocks: collects inline text into a line,
/// and `flush`es it as a block where a block element starts or ends.
struct Writer<'a> {
    dom: &'a Dom,
    /// The page's Address, which links are relative to.
    base: &'a str,
    blocks: Vec<Block>,
    line: String,
    /// White space seen since the last word; written only before the next
    /// one, so that it never ends up inside a marker or at a line's end.
    space: bool,
    spans: Vec<Span>,
    lists: Vec<List>,
    /// The marker of the list item whose first block is next.
    item: Option<String>,
    /// The next block starts a list or table, which no other one continues.
    starts_group: bool,
}

/// Elements that start and end a block.
const BLOCK: &[&str] = &[
    "address",
    "article",
    "body",
    "caption",
    "center",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "form",
    "header",
    "hgroup",
    "html",
    "main",
    "menu",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
];

impl<'a> Writer<'a> {
    fn new(dom: &'a Dom, base: &'a str) -> Self {
        Self {
            dom,
            base,
            blocks: Vec::new(),
            line: String::new(),
            space: false,
            spans: Vec::new(),
            lists: Vec::new(),
            item: None,
            starts_group: false,
        }
    }

    /// Adds a block, tight with the one before if both are items or both rows of one group.
    fn push(&mut self, kind: BlockKind, text: String) {
        let tight = !self.starts_group
            && matches!(kind, BlockKind::Item | BlockKind::Row)
            && self.blocks.last().is_some_and(|b| b.kind == kind);
        self.starts_group = false;
        self.blocks.push(Block { kind, tight, text });
    }

    fn finish(mut self) -> Vec<Block> {
        self.flush("", BlockKind::Paragraph);
        self.blocks
    }

    fn node(&mut self, id: usize) {
        let dom = self.dom;
        let name = match dom.part(id) {
            None => return self.children(id),
            Some(Part::Text(text)) => return self.text(text),
            Some(Part::Element(name)) => name,
        };
        if is_left_out(dom, id) {
            return;
        }
        match name {
            "br" | "hr" => self.flush("", BlockKind::Paragraph),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush("", BlockKind::Paragraph);
                self.children(id);
                let level = (name.as_bytes()[1] - b'0').min(3) as usize;
                self.flush(&format!("{} ", "#".repeat(level)), BlockKind::Other);
            }
            "p" => {
                self.flush("", BlockKind::Paragraph);
                self.children(id);
                self.flush("", BlockKind::Paragraph);
            }
            "ul" | "ol" => {
                self.flush("", BlockKind::Paragraph);
                self.starts_group |= self.lists.is_empty();
                self.lists.push(List {
                    ordered: name == "ol",
                    count: 0,
                });
                self.children(id);
                self.lists.pop();
                self.flush("", BlockKind::Paragraph);
            }
            "li" => {
                self.flush("", BlockKind::Paragraph);
                self.item = Some(self.item_marker());
                self.children(id);
                self.flush("", BlockKind::Paragraph);
                self.item = None;
            }
            "blockquote" => self.quote(id),
            "pre" => self.code_block(id),
            "table" if is_data_table(dom, id) => {
                self.flush("", BlockKind::Paragraph);
                self.starts_group = true;
                self.rows(id);
            }
            "a" => {
                let target = dom
                    .attribute(id, "href")
                    .and_then(|href| resolve(self.base, href))
                    .map(|url| {
                        url.replace(' ', "%20")
                            .replace('(', "%28")
                            .replace(')', "%29")
                    });
                match target {
                    Some(url) => self.span("[", &format!("]({url})"), id),
                    None => self.span("", "", id),
                }
            }
            "b" | "strong" => self.span("**", "**", id),
            "i" | "em" | "cite" => self.span("*", "*", id),
            "code" | "kbd" | "samp" | "tt" => self.span("`", "`", id),
            _ if BLOCK.contains(&name) => {
                self.flush("", BlockKind::Paragraph);
                self.children(id);
                self.flush("", BlockKind::Paragraph);
            }
            _ => self.children(id),
        }
    }

    fn children(&mut self, id: usize) {
        for &child in self.dom.children(id) {
            self.node(child);
        }
    }

    fn span(&mut self, open: &str, close: &str, id: usize) {
        self.spans.push(Span {
            open: open.to_owned(),
            close: close.to_owned(),
            written: false,
        });
        self.children(id);
        if let Some(span) = self.spans.pop().filter(|s| s.written) {
            self.line += &span.close;
        }
    }

    fn text(&mut self, text: &str) {
        let is_space = |c: char| c.is_whitespace();
        self.space |= text.starts_with(is_space);
        for (i, word) in text.split_whitespace().enumerate() {
            self.space |= i > 0;
            if self.space && !self.line.is_empty() {
                self.line.push(' ');
            }
            self.space = false;
            for span in self.spans.iter_mut().filter(|s| !s.written) {
                self.line += &span.open;
                span.written = true;
            }
            self.line += word;
        }
        self.space |= text.ends_with(is_space);
    }

    /// Ends the line as a block; a list item's marker or `prefix` goes before it.
    fn flush(&mut self, prefix: &str, kind: BlockKind) {
        self.space = false;
        if self.line.is_empty() {
            return;
        }
        // A line can't hold a span across blocks: close them, to open again in the next.
        for span in self.spans.iter_mut().rev().filter(|s| s.written) {
            self.line += &span.close;
            span.written = false;
        }
        let (prefix, kind) = match self.item.take() {
            Some(marker) => (format!("{marker}{prefix}"), BlockKind::Item),
            None => (prefix.to_owned(), kind),
        };
        let line = std::mem::take(&mut self.line);
        self.push(kind, prefix + &line);
    }

    fn item_marker(&mut self) -> String {
        let depth = self.lists.len().saturating_sub(1);
        let marker = match self.lists.last_mut() {
            Some(list) if list.ordered => {
                list.count += 1;
                format!("{}. ", list.count)
            }
            _ => "- ".to_owned(),
        };
        format!("{}{marker}", "  ".repeat(depth))
    }

    /// Every line of the blocks inside, quoted.
    fn quote(&mut self, id: usize) {
        self.flush("", BlockKind::Paragraph);
        let mut inner = Writer::new(self.dom, self.base);
        inner.children(id);
        let text = render(&inner.finish());
        let quoted: Vec<_> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| format!("> {l}"))
            .collect();
        if !quoted.is_empty() {
            self.push(BlockKind::Other, quoted.join("\n"));
        }
    }

    fn code_block(&mut self, id: usize) {
        self.flush("", BlockKind::Paragraph);
        fn push(dom: &Dom, id: usize, out: &mut String) {
            match dom.part(id) {
                Some(Part::Text(text)) => out.push_str(text),
                Some(Part::Element("br")) => out.push('\n'),
                Some(Part::Element(_)) if is_left_out(dom, id) => {}
                _ => dom.children(id).iter().for_each(|&c| push(dom, c, out)),
            }
        }
        let mut code = String::new();
        push(self.dom, id, &mut code);
        let code = code.trim_matches('\n').trim_end();
        if !code.trim().is_empty() {
            self.push(BlockKind::Other, format!("```\n{code}\n```"));
        }
    }

    /// One line per row, its cells apart by ` | `.
    fn rows(&mut self, id: usize) {
        for &child in self.dom.children(id) {
            match self.dom.name(child) {
                "tr" => {
                    let cells: Vec<_> = (self.dom.children(child).iter())
                        .filter(|&&c| matches!(self.dom.name(c), "td" | "th"))
                        .map(|&c| {
                            text_of(self.dom, c)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .filter(|cell| !cell.is_empty())
                        .collect();
                    if !cells.is_empty() {
                        self.push(BlockKind::Row, cells.join(" | "));
                    }
                }
                "thead" | "tbody" | "tfoot" => self.rows(child),
                "caption" => self.node(child),
                _ => {}
            }
        }
    }
}

/// Whether a table holds data, which reads as rows, rather than laying out a
/// page, which reads as its cells' content: that has headings or tables in it.
fn is_data_table(dom: &Dom, id: usize) -> bool {
    fn has_layout(dom: &Dom, id: usize) -> bool {
        dom.children(id).iter().any(|&c| {
            matches!(
                dom.name(c),
                "table" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ) || has_layout(dom, c)
        })
    }
    !has_layout(dom, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "https://blog.example.com/2024/post/index.html";

    /// A sentence of prose, `n` times: enough text for a page to read like an article.
    fn prose(n: usize) -> String {
        "The quick brown fox, tired of running, sat down to write about it. ".repeat(n)
    }

    fn markdown(html: &str) -> String {
        read_article(html, ADDRESS).markdown
    }

    #[test]
    fn a_blog_post_is_an_article_without_its_chrome() {
        let html = format!(
            r#"<html><head><title>T</title><script>var x = "<p>no</p>";</script></head><body>
            <header class="site-header"><nav><a href="/">Home</a><a href="/about">About</a></nav></header>
            <div id="content"><article class="post">
              <h1>On foxes</h1>
              <p>{}</p>
              <p>Second <em>paragraph</em> with <strong>weight</strong> and <code>code</code>.</p>
            </article></div>
            <aside class="sidebar"><p>{}</p></aside>
            <footer><p>Copyright</p></footer></body></html>"#,
            prose(8),
            prose(8)
        );
        let article = read_article(&html, ADDRESS);
        assert!(article.reads_like_article);
        assert_eq!(
            article.markdown,
            format!(
                "# On foxes\n\n{}\n\nSecond *paragraph* with **weight** and `code`.\n",
                prose(8).trim()
            )
        );
    }

    #[test]
    fn a_news_page_leaves_out_ads_comments_banners_and_footers() {
        let html = format!(
            r#"<body>
            <div class="cookie-banner"><p>We use cookies, many cookies, all the cookies, honestly.</p></div>
            <div class="ad-slot"><p>Buy this, buy that, buy everything, and be happy forever.</p></div>
            <div class="story-body"><h2>Headline</h2><p>{}</p><p>{}</p>
              <div class="share-tools"><p>Share this story with your friends, family, and enemies.</p></div>
            </div>
            <section id="comments"><h3>Comments</h3><p>{}</p><div class="comment"><p>First! {}</p></div></section>
            <footer>Contact us</footer></body>"#,
            prose(5),
            prose(5),
            prose(9),
            prose(2)
        );
        let article = read_article(&html, ADDRESS);
        assert!(article.reads_like_article);
        assert_eq!(
            article.markdown,
            format!(
                "## Headline\n\n{}\n\n{}\n",
                prose(5).trim(),
                prose(5).trim()
            )
        );
    }

    #[test]
    fn a_docs_page_keeps_headings_lists_code_and_tables() {
        let html = format!(
            r#"<main><h1>Guide</h1><p>{}</p>
            <h2>Install</h2><h4>Deep</h4><h6>Deeper</h6>
            <pre><code>cargo add x
  indented()</code></pre>
            <table><thead><tr><th>Name</th><th>Type</th></tr></thead>
              <tbody><tr><td>id</td><td>u32</td></tr><tr><td>name</td><td></td></tr></tbody></table>
            <p>{}</p></main>"#,
            prose(5),
            prose(5)
        );
        let article = read_article(&html, ADDRESS);
        assert!(article.reads_like_article);
        assert_eq!(
            article.markdown,
            format!(
                "# Guide\n\n{}\n\n## Install\n\n### Deep\n\n### Deeper\n\n```\ncargo add x\n  indented()\n```\n\nName | Type\nid | u32\nname\n\n{}\n",
                prose(5).trim(),
                prose(5).trim()
            )
        );
    }

    #[test]
    fn nested_lists_keep_their_depth_and_numbering() {
        let html = "<ul><li>one<ul><li>inner</li><li>inner two</li></ul></li><li>two</li></ul>\
                    <ol><li><p>a</p></li><li>b<ol><li>c</li></ol></li></ol>";
        assert_eq!(
            markdown(html),
            "- one\n  - inner\n  - inner two\n- two\n\n1. a\n2. b\n  1. c\n"
        );
    }

    #[test]
    fn links_are_absolute_and_quotes_are_quoted() {
        let html = r##"<p>See <a href="../other.html">this <b>page</b></a>, <a href="/root">root</a>,
            <a href="https://x.test/a b(1)">far</a>, <a href="#top">top</a>, <a href="javascript:void(0)">js</a>
            and <a href="mailto:a@b.c">mail</a>.</p>
            <blockquote><p>Wise words.</p><p>More.</p></blockquote>"##;
        assert_eq!(
            markdown(html),
            "See [this **page**](https://blog.example.com/2024/other.html), [root](https://blog.example.com/root), \
             [far](https://x.test/a%20b%281%29), top, js and mail.\n\n> Wise words.\n> More.\n"
        );
    }

    #[test]
    fn markers_never_wrap_white_space_or_nothing() {
        assert_eq!(
            markdown("<p>a <b> bold </b> b <i></i><a href='/x'><img src=x></a>c</p>"),
            "a **bold** b c\n"
        );
    }

    #[test]
    fn a_link_over_blocks_is_one_link_per_block() {
        assert_eq!(
            markdown(r#"<a href="/x"><div>one</div><div>two</div></a>"#),
            "[one](https://blog.example.com/x)\n\n[two](https://blog.example.com/x)\n"
        );
    }

    #[test]
    fn a_page_of_links_is_no_article_but_keeps_its_text() {
        let teaser = |n: u32| {
            format!(
                r#"<li><h3><a href="/story/{n}">Story number {n} has a headline</a></h3><p>A short blurb about story {n}, nothing more.</p></li>"#
            )
        };
        let html = format!(
            "<body><nav><a href='/'>Home</a></nav><ul>{}</ul></body>",
            (1..9).map(teaser).collect::<String>()
        );
        let article = read_article(&html, ADDRESS);
        assert!(!article.reads_like_article);
        assert!(
            article
                .markdown
                .contains("### [Story number 3 has a headline](https://blog.example.com/story/3)")
        );
    }

    #[test]
    fn a_short_page_is_no_article() {
        let article = read_article(
            "<body><p>Under construction, come back later, please.</p></body>",
            ADDRESS,
        );
        assert!(!article.reads_like_article);
        assert_eq!(
            article.markdown,
            "Under construction, come back later, please.\n"
        );
    }

    #[test]
    fn broken_markup_gives_what_can_be_read() {
        let html = format!(
            "<div><p>{}<div><b>unclosed <p>{}</i></span><ul><li>x<li>y",
            prose(5),
            prose(5)
        );
        let article = read_article(&html, ADDRESS);
        assert!(article.reads_like_article);
        // The `<b>` nobody closed holds the rest of the page.
        assert!(article.markdown.ends_with("- **x**\n- **y**\n"));
        assert_eq!(read_article("", ADDRESS), Article::default());
        assert_eq!(
            read_article("<<<>>> &&& </", ADDRESS).markdown,
            "<<<>>> &&&\n"
        );
    }

    #[test]
    fn layout_tables_read_as_their_content() {
        let html = format!(
            "<table><tr><td><h2>Title</h2><p>{}</p></td></tr></table>",
            prose(1)
        );
        assert_eq!(
            markdown(&html),
            format!("## Title\n\n{}\n", prose(1).trim())
        );
    }

    #[test]
    fn hidden_elements_are_left_out() {
        assert_eq!(
            markdown(
                r#"<p>a</p><p hidden>b</p><p style="display: none">c</p><div aria-hidden="true">d</div><p>e</p>"#
            ),
            "a\n\ne\n"
        );
    }
}
