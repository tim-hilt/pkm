//! Details of a page (Bookmarks ADR-0001): the Title, Site, Cover, Excerpt,
//! Author and Published a page gives about itself in `<title>`, Open Graph,
//! `<meta>` tags and JSON-LD. Pure: the client downloads, this only reads.
//! Tolerates broken markup: what can't be read is left out.

use crate::address::{domain, resolve};
use crate::html::{attributes, clean, find_ignoring_case};
use pkm_base::json::{self, Value};

/// What a page says about itself. Empty where it says nothing, and for the
/// Title and Site then what an Address alone tells: the Address and its domain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub title: String,
    pub site: String,
    /// Where the Cover is, as an absolute address.
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    /// As the page gives it, usually ISO 8601.
    pub published: String,
}

/// The Details of the page at `address` whose HTML is `html`. Empty `html`
/// (a download that failed, or none tried) gives the Details of the Address alone.
pub fn extract(html: &str, address: &str) -> Details {
    let page = Page::scan(html);
    let nodes = page.json_ld_nodes();
    let ld = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| nodes.iter().find_map(|n| ld_text(n, key)))
    };
    let meta = |keys: &[&str]| keys.iter().find_map(|key| page.meta(key));
    let first = |found: Option<String>| found.unwrap_or_default();

    let title = [
        meta(&["og:title", "twitter:title"]),
        ld(&["headline"]),
        page.title.clone(),
    ]
    .into_iter()
    .flatten()
    .next();
    let site = [
        meta(&["og:site_name", "application-name"]),
        ld(&["publisher"]),
    ]
    .into_iter()
    .flatten()
    .next();
    let cover = [
        meta(&["og:image", "og:image:url", "twitter:image"]),
        ld(&["image"]),
    ]
    .into_iter()
    .flatten()
    .find_map(|image| resolve(address, &image));
    let author = meta(&["author"]).or_else(|| ld(&["author"]));
    // `article:author` is usually a profile address, no name.
    let author = author.or_else(|| meta(&["article:author"]).filter(|a| !a.contains("://")));
    Details {
        title: title.unwrap_or_else(|| address.to_owned()),
        site: site.unwrap_or_else(|| domain(address)),
        cover,
        excerpt: first(
            meta(&["og:description", "description", "twitter:description"])
                .or_else(|| ld(&["description"])),
        ),
        author: first(author),
        published: first(
            meta(&["article:published_time", "datepublished", "date", "dc.date"])
                .or_else(|| ld(&["datePublished"])),
        ),
    }
}

/// What the scan of a page found.
#[derive(Default)]
struct Page {
    title: Option<String>,
    /// `<meta>` tags as (lower-cased `property`/`name`/`itemprop`, `content`).
    metas: Vec<(String, String)>,
    /// The text of `<script type="application/ld+json">`s.
    json_ld: Vec<String>,
}

impl Page {
    fn scan(html: &str) -> Self {
        let mut page = Self::default();
        let mut at = 0;
        while let Some(open) = html[at..].find('<') {
            let start = at + open + 1;
            let rest = &html[start..];
            at = start;
            if let Some(comment) = rest.strip_prefix("!--") {
                at = start + 3 + comment.find("-->").map_or(comment.len(), |end| end + 3);
                continue;
            }
            if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                // `</x>`, `<!doctype>` and `<?xml?>` end at `>`; any other `<` is text.
                if rest.starts_with(['/', '!', '?']) {
                    at = start + rest.find('>').map_or(rest.len(), |end| end + 1);
                }
                continue;
            }
            let name_end = rest
                .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .unwrap_or(rest.len());
            let name = rest[..name_end].to_ascii_lowercase();
            let (attributes, used) = attributes(&rest[name_end..]);
            at = start + name_end + used;
            let attribute = |key: &str| {
                attributes
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
            };
            match name.as_str() {
                "meta" => {
                    let key = attribute("property")
                        .or(attribute("name"))
                        .or(attribute("itemprop"));
                    if let (Some(key), Some(content)) = (key, attribute("content")) {
                        page.metas.push((key.to_ascii_lowercase(), clean(content)));
                    }
                }
                "title" | "script" | "style" => {
                    let end = find_ignoring_case(&html[at..], &format!("</{name}"))
                        .unwrap_or(html.len() - at);
                    let text = &html[at..at + end];
                    at += end;
                    match name.as_str() {
                        "title" if page.title.is_none() => {
                            page.title = Some(clean(text)).filter(|t| !t.is_empty())
                        }
                        "script"
                            if attribute("type")
                                .is_some_and(|t| t.to_ascii_lowercase().contains("ld+json")) =>
                        {
                            page.json_ld.push(text.to_owned());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        page
    }

    /// The first non-empty `<meta>` content with this key (lower case).
    fn meta(&self, key: &str) -> Option<String> {
        (self.metas.iter())
            .find(|(k, v)| k == key && !v.is_empty())
            .map(|(_, v)| v.clone())
    }

    /// Every JSON-LD object on the page, those in a `@graph` too.
    fn json_ld_nodes(&self) -> Vec<Value> {
        let mut nodes = Vec::new();
        for text in &self.json_ld {
            let Some(value) = json::parse_bytes(text.trim().as_bytes()) else {
                continue;
            };
            let mut pending = vec![value];
            while let Some(value) = pending.pop() {
                match value {
                    Value::Array(items) => pending.extend(items.into_iter().rev()),
                    Value::Object(_) => {
                        if let Some(Value::Array(graph)) = value.get("@graph") {
                            pending.extend(graph.iter().rev().cloned());
                        }
                        nodes.push(value);
                    }
                    _ => {}
                }
            }
        }
        nodes
    }
}

/// A JSON-LD member as text: a string, the `name` or `url` of an object, or
/// those of an array's items joined by commas.
fn ld_text(node: &Value, key: &str) -> Option<String> {
    fn text(value: &Value) -> Option<String> {
        match value {
            Value::String(s) => Some(clean(s)),
            Value::Object(_) => ["name", "url"].iter().find_map(|k| text(value.get(k)?)),
            Value::Array(items) => {
                let texts: Vec<_> = items.iter().filter_map(text).collect();
                Some(texts.join(", "))
            }
            _ => None,
        }
        .filter(|t| !t.is_empty())
    }
    text(node.get(key)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "https://www.example.com/blog/post";

    #[test]
    fn open_graph_gives_every_detail() {
        let html = r#"<!doctype html><html><head>
            <title>Fallback title</title>
            <meta property="og:title" content="Rust &amp; Me">
            <meta property="og:site_name" content="The Blog">
            <meta property="og:image" content="/img/cover.png">
            <meta property="og:description" content="  A short
               story ">
            <meta name="author" content="Ada Lovelace">
            <meta property="article:published_time" content="2024-05-01T10:00:00Z">
            </head><body>x</body></html>"#;
        assert_eq!(
            extract(html, ADDRESS),
            Details {
                title: "Rust & Me".into(),
                site: "The Blog".into(),
                cover: Some("https://www.example.com/img/cover.png".into()),
                excerpt: "A short story".into(),
                author: "Ada Lovelace".into(),
                published: "2024-05-01T10:00:00Z".into(),
            }
        );
    }

    #[test]
    fn plain_meta_tags_and_title_serve_where_open_graph_is_missing() {
        let html = "<TITLE>\n  Plain   page </TITLE><META NAME='description' CONTENT='About it'><meta name=author content=Bob>";
        let details = extract(html, ADDRESS);
        assert_eq!(
            (
                details.title.as_str(),
                details.excerpt.as_str(),
                details.author.as_str()
            ),
            ("Plain page", "About it", "Bob")
        );
    }

    #[test]
    fn json_ld_serves_where_meta_tags_are_missing_even_inside_a_graph() {
        let html = r#"<head><script type="application/ld+json">
            {"@context":"https://schema.org","@graph":[
              {"@type":"WebSite","name":"ignored"},
              {"@type":"Article","headline":"LD headline",
               "author":[{"@type":"Person","name":"Ann"},{"name":"Ben"}],
               "datePublished":"2023-01-02","description":"LD text",
               "image":{"@type":"ImageObject","url":"https://cdn.test/c.jpg"},
               "publisher":{"@type":"Organization","name":"LD News"}}]}
            </script></head>"#;
        assert_eq!(
            extract(html, ADDRESS),
            Details {
                title: "LD headline".into(),
                site: "LD News".into(),
                cover: Some("https://cdn.test/c.jpg".into()),
                excerpt: "LD text".into(),
                author: "Ann, Ben".into(),
                published: "2023-01-02".into(),
            }
        );
    }

    #[test]
    fn missing_metadata_falls_back_to_the_address() {
        let details = extract("<p>No head at all</p>", ADDRESS);
        assert_eq!(details.title, ADDRESS);
        assert_eq!(details.site, "example.com");
        assert_eq!(
            (
                details.cover,
                details.excerpt,
                details.author,
                details.published
            ),
            (None, String::new(), String::new(), String::new())
        );
        assert_eq!(extract("", ADDRESS), details_of_address_only());
    }

    fn details_of_address_only() -> Details {
        Details {
            title: ADDRESS.into(),
            site: "example.com".into(),
            ..Details::default()
        }
    }

    #[test]
    fn a_profile_address_is_no_author_name() {
        let html = r#"<meta property="article:author" content="https://example.com/ann">"#;
        assert_eq!(extract(html, ADDRESS).author, "");
    }

    #[test]
    fn broken_markup_gives_what_can_be_read() {
        let html = concat!(
            "<html><head><title>Still here",
            "<meta property=og:site_name content=Site>",
            "<meta content='cut off"
        );
        // The unclosed title swallows the rest, as a browser's does.
        assert_eq!(extract(html, ADDRESS).site, "example.com");
        let html = concat!(
            "<<< <meta property=\"og:site_name\" content=\"Site\" <b> ",
            "<!-- <meta property=\"og:title\" content=\"hidden\"> --> ",
            "<script>var a = '<meta property=\"og:title\" content=\"in script\">';</script>",
            "<script type=\"application/ld+json\">{broken</script><meta =x content=y>"
        );
        let details = extract(html, ADDRESS);
        assert_eq!(
            (details.title.as_str(), details.site.as_str()),
            (ADDRESS, "Site")
        );
    }

    #[test]
    fn a_cover_that_is_no_web_address_is_left_out() {
        let html = r#"<meta property="og:image" content="data:image/png;base64,AAAA">"#;
        assert_eq!(extract(html, ADDRESS).cover, None);
    }
}
