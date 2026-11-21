//! Addresses (GLOSSARY.md): where a page is on the web. Only `http` and
//! `https` ones are Addresses.

/// An address in its parts; the `#` part is dropped at once, as no Address
/// keeps it.
struct Parts<'a> {
    scheme: &'a str,
    /// `user@host:port`
    authority: &'a str,
    /// Starts with `/`, or is empty.
    path: &'a str,
    query: Option<&'a str>,
}

impl<'a> Parts<'a> {
    fn of(address: &'a str) -> Option<Self> {
        let address = address.trim();
        let address = address
            .split_once('#')
            .map_or(address, |(before, _)| before);
        let (scheme, rest) = address.split_once("://")?;
        if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
            return None;
        }
        let (rest, query) = match rest.split_once('?') {
            Some((rest, query)) => (rest, Some(query)),
            None => (rest, None),
        };
        let at = rest.find('/').unwrap_or(rest.len());
        let (authority, path) = rest.split_at(at);
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        if host.is_empty() || host.starts_with(':') || authority.contains(char::is_whitespace) {
            return None;
        }
        Some(Self {
            scheme,
            authority,
            path,
            query,
        })
    }

    /// The authority with its host in lower case.
    fn authority_lowercased(&self) -> String {
        match self.authority.rsplit_once('@') {
            Some((user, host)) => format!("{user}@{}", host.to_lowercase()),
            None => self.authority.to_lowercase(),
        }
    }
}

/// Parameters that tell where a visitor came from, not which page it is.
fn is_tracking(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid"
                | "gclid"
                | "dclid"
                | "msclkid"
                | "yclid"
                | "igshid"
                | "mc_cid"
                | "mc_eid"
                | "mkt_tok"
                | "_ga"
                | "_gl"
        )
}

/// The Address `address` stands for: tracking parameters and the `#` part
/// gone, scheme and host in lower case, `/` for an empty path. Two spellings
/// of one page give one Address. `None` if it is no `http` or `https` address.
pub fn normalise(address: &str) -> Option<String> {
    let parts = Parts::of(address)?;
    let mut out = format!(
        "{}://{}{}",
        parts.scheme.to_ascii_lowercase(),
        parts.authority_lowercased(),
        if parts.path.is_empty() {
            "/"
        } else {
            parts.path
        }
    );
    let kept: Vec<&str> = (parts.query.unwrap_or_default().split('&'))
        .filter(|pair| !pair.is_empty() && !is_tracking(pair.split('=').next().unwrap_or("")))
        .collect();
    if !kept.is_empty() {
        out += &format!("?{}", kept.join("&"));
    }
    Some(out)
}

/// Whether the Address names a PDF or EPUB, which is Saved without an Article.
pub fn is_document(address: &str) -> bool {
    Parts::of(address).is_some_and(|parts| {
        let path = parts.path.to_ascii_lowercase();
        path.ends_with(".pdf") || path.ends_with(".epub")
    })
}

/// The domain of the page at `address` without a leading `www.`; empty for no Address.
pub(crate) fn domain(address: &str) -> String {
    let Some(parts) = Parts::of(address) else {
        return String::new();
    };
    let host = parts
        .authority
        .rsplit_once('@')
        .map_or(parts.authority, |(_, h)| h);
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit()))
        .map_or(host, |(h, _)| h);
    let host = host.to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// The absolute `http(s)` address that `reference` (as in `src` or `content`)
/// means on the page at `base`, its `#` part kept; `None` for anything else
/// (`data:`, ...) and for a place on the page itself (`#top`).
pub(crate) fn resolve(base: &str, reference: &str) -> Option<String> {
    let (reference, fragment) = match reference.trim().split_once('#') {
        Some((reference, fragment)) => (reference, format!("#{fragment}")),
        None => (reference.trim(), String::new()),
    };
    let base = Parts::of(base)?;
    let origin = format!("{}://{}", base.scheme, base.authority);
    let joined = if reference.contains("://") {
        reference.to_owned()
    } else if let Some(rest) = reference.strip_prefix("//") {
        format!("{}://{rest}", base.scheme)
    } else if reference.starts_with('/') {
        format!("{origin}{reference}")
    } else if reference.is_empty() || reference.starts_with('?') || reference.contains(':') {
        return None;
    } else {
        let directory = base.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        format!("{origin}{directory}/{reference}")
    };
    Parts::of(&joined).map(|parts| {
        format!(
            "{}://{}{}{}{fragment}",
            parts.scheme,
            parts.authority,
            dot_segments(parts.path),
            parts.query.map(|q| format!("?{q}")).unwrap_or_default()
        )
    })
}

/// `path` with its `.` and `..` segments applied.
fn dot_segments(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/').skip(1) {
        match segment {
            "." => {}
            ".." => {
                out.pop();
            }
            segment => out.push(segment),
        }
    }
    format!("/{}", out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_parameters_and_the_hash_part_are_stripped() {
        assert_eq!(
            normalise("https://example.com/a?utm_source=x&id=7&fbclid=abc&UTM_medium=y#top"),
            Some("https://example.com/a?id=7".into())
        );
        assert_eq!(
            normalise("https://example.com/a?utm_source=x"),
            Some("https://example.com/a".into())
        );
    }

    #[test]
    fn parameters_that_name_the_page_are_kept_in_order() {
        for kept in [
            "https://youtube.com/watch?v=abc&t=10",
            "https://example.com/search?q=utm&page=2",
            "https://example.com/p?ref=main",
        ] {
            assert_eq!(normalise(kept), Some(kept.into()));
        }
    }

    #[test]
    fn scheme_and_host_are_lowercased_but_path_and_userinfo_are_not() {
        assert_eq!(
            normalise("  HTTPS://Example.COM:8080/Some/Path "),
            Some("https://example.com:8080/Some/Path".into())
        );
        assert_eq!(
            normalise("http://Me@Example.com"),
            Some("http://Me@example.com/".into())
        );
    }

    #[test]
    fn only_web_addresses_are_addresses() {
        for no in [
            "",
            "example.com",
            "ftp://example.com/a",
            "file:///etc/passwd",
            "https://",
            "https:///a",
        ] {
            assert_eq!(normalise(no), None, "{no}");
        }
    }

    #[test]
    fn pdf_and_epub_addresses_are_documents() {
        assert!(is_document("https://example.com/paper.PDF?download=1"));
        assert!(is_document("https://example.com/book.epub#x"));
        assert!(!is_document("https://example.com/pdf"));
    }

    #[test]
    fn the_domain_has_no_www_port_or_user() {
        assert_eq!(domain("https://User@WWW.Example.com:8080/a"), "example.com");
        assert_eq!(domain("nonsense"), "");
    }

    #[test]
    fn references_resolve_against_the_page() {
        let base = "https://example.com/blog/post.html?x=1";
        for (reference, expected) in [
            ("https://cdn.test/a.png", Some("https://cdn.test/a.png")),
            ("//cdn.test/a.png", Some("https://cdn.test/a.png")),
            ("/img/a.png", Some("https://example.com/img/a.png")),
            ("a.png", Some("https://example.com/blog/a.png")),
            ("../a.png", Some("https://example.com/a.png")),
            (
                "./x/../a.png?v=2",
                Some("https://example.com/blog/a.png?v=2"),
            ),
            ("https://x.test/a#part", Some("https://x.test/a#part")),
            ("/b#c", Some("https://example.com/b#c")),
            ("data:image/png;base64,AAAA", None),
            ("#top", None),
            ("", None),
        ] {
            assert_eq!(resolve(base, reference).as_deref(), expected, "{reference}");
        }
    }
}
