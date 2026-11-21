//! Reading HTML: tags, attributes and character references. Tolerates broken markup.

/// The attributes at the start of `text`, which follows a tag's name, as
/// (lower-cased name, decoded value), and how many bytes the tag's rest takes,
/// its `>` included.
pub(crate) fn attributes(text: &str) -> (Vec<(String, String)>, usize) {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    let skip_space = |mut i: usize| {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    };
    while i < bytes.len() {
        i = skip_space(i);
        match bytes.get(i) {
            None => break,
            Some(b'>') => return (found, i + 1),
            Some(b'/') => {
                i += 1;
                continue;
            }
            _ => {}
        }
        let name_start = i;
        // A name starting with `=` is broken markup; take it whole to move on.
        i += 1;
        while i < bytes.len()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
            && !bytes[i].is_ascii_whitespace()
        {
            i += 1;
        }
        let name = text[name_start..i].to_ascii_lowercase();
        i = skip_space(i);
        let mut value = "";
        if bytes.get(i) == Some(&b'=') {
            i = skip_space(i + 1);
            match bytes.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let end = text[i + 1..]
                        .find(quote as char)
                        .map_or(text.len(), |e| i + 1 + e);
                    value = &text[i + 1..end];
                    i = (end + 1).min(text.len());
                }
                _ => {
                    let start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    value = &text[start..i];
                }
            }
        }
        found.push((name, decode(value)));
    }
    (found, text.len())
}

/// The position of `needle` (lower case) in `haystack`, ignoring ASCII case.
pub(crate) fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// `text` with its character references decoded and white space collapsed.
pub(crate) fn clean(text: &str) -> String {
    decode(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` with the character references that pages use decoded; any other left as is.
pub(crate) fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out += &rest[..amp];
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let character = match name.strip_prefix('#') {
                Some(number) => {
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16),
                        None => number.parse(),
                    };
                    char::from_u32(code.ok()?)?
                }
                None => NAMED.iter().find(|(n, _)| *n == name)?.1,
            };
            Some((character, end + 1))
        });
        match decoded {
            Some((character, used)) => {
                out.push(character);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out + rest
}

/// The named character references pages use in text; the full list of HTML
/// has over 2000.
const NAMED: &[(&str, char)] = &[
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("shy", '\u{ad}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("sbquo", '‚'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bdquo", '„'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("lsaquo", '‹'),
    ("rsaquo", '›'),
    ("hellip", '…'),
    ("bull", '•'),
    ("middot", '·'),
    ("prime", '′'),
    ("Prime", '″'),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("deg", '°'),
    ("sect", '§'),
    ("para", '¶'),
    ("dagger", '†'),
    ("Dagger", '‡'),
    ("times", '×'),
    ("divide", '÷'),
    ("plusmn", '±'),
    ("minus", '−'),
    ("le", '≤'),
    ("ge", '≥'),
    ("ne", '≠'),
    ("asymp", '≈'),
    ("infin", '∞'),
    ("frac12", '½'),
    ("frac14", '¼'),
    ("frac34", '¾'),
    ("larr", '←'),
    ("rarr", '→'),
    ("uarr", '↑'),
    ("darr", '↓'),
    ("harr", '↔'),
    ("euro", '€'),
    ("pound", '£'),
    ("yen", '¥'),
    ("cent", '¢'),
    ("iexcl", '¡'),
    ("iquest", '¿'),
    ("szlig", 'ß'),
    ("auml", 'ä'),
    ("ouml", 'ö'),
    ("uuml", 'ü'),
    ("Auml", 'Ä'),
    ("Ouml", 'Ö'),
    ("Uuml", 'Ü'),
    ("aacute", 'á'),
    ("eacute", 'é'),
    ("iacute", 'í'),
    ("oacute", 'ó'),
    ("uacute", 'ú'),
    ("agrave", 'à'),
    ("egrave", 'è'),
    ("ecirc", 'ê'),
    ("ccedil", 'ç'),
    ("ntilde", 'ñ'),
];

/// A page as a tree. Node 0 is the root; every other node's `parent` is the
/// node it was found in.
pub(crate) struct Dom {
    nodes: Vec<Node>,
}

struct Node {
    kind: NodeKind,
    parent: usize,
    children: Vec<usize>,
}

enum NodeKind {
    Root,
    /// Character references decoded; white space as in the page.
    Text(String),
    Element {
        /// Lower case.
        name: String,
        /// (lower-cased name, decoded value)
        attributes: Vec<(String, String)>,
    },
}

/// What a node is.
pub(crate) enum Part<'a> {
    Text(&'a str),
    Element(&'a str),
}

/// Elements without content or closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose content is no markup; it is left out of the tree.
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];

/// Elements that end an open `<p>`.
const ENDS_PARAGRAPH: &[&str] = &[
    "address",
    "article",
    "blockquote",
    "div",
    "dl",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "main",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

/// Open elements nested deeper than this are taken as empty, so that broken
/// markup can't make the tree too deep to walk.
const MAX_DEPTH: usize = 200;

impl Dom {
    pub(crate) const ROOT: usize = 0;

    /// The tree of `html`. Closes what is left open, ignores end tags that
    /// close nothing, and applies the implied ends of `<p>`, `<li>`, `<dt>`,
    /// `<dd>`, `<tr>`, `<td>` and `<th>`.
    pub(crate) fn parse(html: &str) -> Self {
        let mut dom = Self {
            nodes: vec![Node {
                kind: NodeKind::Root,
                parent: Self::ROOT,
                children: Vec::new(),
            }],
        };
        let mut open = vec![Self::ROOT];
        let mut at = 0;
        while at < html.len() {
            let Some(lt) = html[at..].find('<') else {
                dom.add_text(&open, &html[at..]);
                break;
            };
            dom.add_text(&open, &html[at..at + lt]);
            let start = at + lt + 1;
            let rest = &html[start..];
            at = start;
            if let Some(comment) = rest.strip_prefix("!--") {
                at = start + 3 + comment.find("-->").map_or(comment.len(), |end| end + 3);
            } else if let Some(end_tag) = rest.strip_prefix('/') {
                let end = end_tag.find('>').map_or(end_tag.len(), |end| end + 1);
                let name = end_tag[..end].trim_end_matches('>').trim();
                dom.close(&mut open, &name.to_ascii_lowercase());
                at = start + 1 + end;
            } else if rest.starts_with(['!', '?']) {
                at = start + rest.find('>').map_or(rest.len(), |end| end + 1);
            } else if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                dom.add_text(&open, "<");
            } else {
                let name_end = rest
                    .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                    .unwrap_or(rest.len());
                let name = rest[..name_end].to_ascii_lowercase();
                let (attributes, used) = attributes(&rest[name_end..]);
                at = start + name_end + used;
                if RAW_TEXT.contains(&name.as_str()) {
                    at += find_ignoring_case(&html[at..], &format!("</{name}"))
                        .unwrap_or(html.len() - at);
                    continue;
                }
                dom.open(&mut open, name, attributes);
            }
        }
        dom
    }

    fn add_text(&mut self, open: &[usize], text: &str) {
        if text.is_empty() {
            return;
        }
        let parent = open[open.len() - 1];
        let last = self.nodes[parent].children.last().copied();
        match last.map(|id| &mut self.nodes[id].kind) {
            Some(NodeKind::Text(before)) => before.push_str(&decode(text)),
            _ => {
                self.add(parent, NodeKind::Text(decode(text)));
            }
        }
    }

    fn add(&mut self, parent: usize, kind: NodeKind) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent,
            children: Vec::new(),
        });
        self.nodes[parent].children.push(id);
        id
    }

    fn open(&mut self, open: &mut Vec<usize>, name: String, attributes: Vec<(String, String)>) {
        match name.as_str() {
            "li" => self.close_first(open, &["li"], &["ul", "ol"]),
            "dt" | "dd" => self.close_first(open, &["dt", "dd"], &["dl"]),
            "tr" => self.close_first(open, &["tr"], &["table"]),
            "td" | "th" => self.close_first(open, &["td", "th"], &["tr", "table"]),
            _ => {}
        }
        if ENDS_PARAGRAPH.contains(&name.as_str()) && self.name(open[open.len() - 1]) == "p" {
            open.pop();
        }
        let void = VOID.contains(&name.as_str());
        let id = self.add(open[open.len() - 1], NodeKind::Element { name, attributes });
        if !void && open.len() < MAX_DEPTH {
            open.push(id);
        }
    }

    /// Closes the innermost open element called `name`, with all inside it,
    /// if there is one.
    fn close(&self, open: &mut Vec<usize>, name: &str) {
        if let Some(at) = open.iter().rposition(|&id| self.name(id) == name) {
            open.truncate(at.max(1));
        }
    }

    /// Closes the innermost open element among `names` unless one of `barriers` is inside it.
    fn close_first(&self, open: &mut Vec<usize>, names: &[&str], barriers: &[&str]) {
        for at in (1..open.len()).rev() {
            let name = self.name(open[at]);
            if barriers.contains(&name) {
                return;
            }
            if names.contains(&name) {
                open.truncate(at);
                return;
            }
        }
    }

    /// The node's name if it is an element, else "".
    pub(crate) fn name(&self, id: usize) -> &str {
        match &self.nodes[id].kind {
            NodeKind::Element { name, .. } => name,
            _ => "",
        }
    }

    pub(crate) fn attribute(&self, id: usize, key: &str) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Element { attributes, .. } => attributes
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    pub(crate) fn parent(&self, id: usize) -> usize {
        self.nodes[id].parent
    }

    pub(crate) fn children(&self, id: usize) -> &[usize] {
        &self.nodes[id].children
    }

    pub(crate) fn part(&self, id: usize) -> Option<Part<'_>> {
        match &self.nodes[id].kind {
            NodeKind::Root => None,
            NodeKind::Text(text) => Some(Part::Text(text)),
            NodeKind::Element { name, .. } => Some(Part::Element(name)),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_references_are_decoded() {
        assert_eq!(
            decode("a&amp;b &lt;&#65;&#x42;&gt; &nbsp;&bogus; & &rsquo;&mdash;&hellip;"),
            "a&b <AB> \u{a0}&bogus; & ’—…"
        );
    }

    /// The tree as nested `name(children)` with text in quotes.
    fn shape(dom: &Dom, id: usize) -> String {
        let inner: Vec<_> = dom.children(id).iter().map(|&c| shape(dom, c)).collect();
        match dom.part(id) {
            Some(Part::Text(text)) => format!("{text:?}"),
            Some(Part::Element(name)) => format!("{name}({})", inner.join(" ")),
            None => inner.join(" "),
        }
    }

    fn parsed(html: &str) -> String {
        shape(&Dom::parse(html), Dom::ROOT)
    }

    #[test]
    fn nests_elements_and_decodes_text() {
        assert_eq!(
            parsed("<div class=a>x &amp; <b>y</b><br>z</div>"),
            r#"div("x & " b("y") br() "z")"#
        );
    }

    #[test]
    fn scripts_styles_and_comments_leave_no_trace() {
        assert_eq!(
            parsed("<p>a<script>if (a < b) { '</p>' }</script><!-- <b> -->b<style>p{}</style></p>"),
            r#"p("ab")"#
        );
    }

    #[test]
    fn broken_markup_is_closed_and_stray_end_tags_ignored() {
        assert_eq!(parsed("</b><div><i>a</div>b</span>"), r#"div(i("a")) "b""#);
        assert_eq!(
            parsed("<div><p>a<p>b<ul><li>c<li>d</ul>"),
            r#"div(p("a") p("b") ul(li("c") li("d")))"#
        );
        assert_eq!(parsed("a < b <3"), r#""a < b <3""#);
    }

    #[test]
    fn table_cells_and_rows_end_each_other() {
        assert_eq!(
            parsed("<table><tr><td>a<td>b<tr><td>c</table>"),
            r#"table(tr(td("a") td("b")) tr(td("c")))"#
        );
    }

    #[test]
    fn absurdly_deep_markup_stays_shallow() {
        let dom = Dom::parse(&"<div>".repeat(10_000));
        assert!(dom.len() > MAX_DEPTH);
        let mut depth = 0;
        let mut id = Dom::ROOT;
        while let Some(&child) = dom.children(id).first() {
            id = child;
            depth += 1;
        }
        assert!(depth <= MAX_DEPTH + 1);
    }
}
