//! Image references in an Article's Markdown, `![alt](address)`: the one place
//! that knows their syntax. `read_article` writes them with absolute
//! addresses; Save rewrites them to the files kept beside the Bookmark.

/// A Markdown image reference to `address`, which has no spaces or brackets
/// that would end it early. `alt` is flattened to one line without brackets.
pub(crate) fn reference(alt: &str, address: &str) -> String {
    let alt: String = alt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['[', ']'], "");
    format!("![{alt}]({})", escape(address))
}

/// `address` made safe as the target of a Markdown link or image.
pub(crate) fn escape(address: &str) -> String {
    address
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// The references in `markdown`, outside fenced code, each replaced by what
/// `target` makes of its address: `Some(address)` keeps it pointing there,
/// `None` drops it. A line left empty by a dropped reference goes with the
/// blank line after it, so that no gap is left in the text.
pub(crate) fn rewrite(markdown: &str, mut target: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_code = false;
    let mut lines = markdown.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if in_code || !line.contains("![") {
            out += line;
            continue;
        }
        let body = line.trim_end_matches('\n');
        let rewritten = rewrite_line(body, &mut target);
        let emptied = rewritten != body && rewritten.trim_matches(['>', ' ']).is_empty();
        if emptied {
            if lines.peek().is_some_and(|next| next.trim().is_empty()) {
                lines.next();
            }
            continue;
        }
        out += &rewritten;
        out += &line[body.len()..];
    }
    out
}

fn rewrite_line(line: &str, target: &mut impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("![") {
        let after = &rest[start + 2..];
        let found = after.find("](").and_then(|alt_end| {
            let alt = &after[..alt_end];
            let address = &after[alt_end + 2..];
            let end = address.find(')')?;
            (!alt.contains(']')).then_some((alt, &address[..end], alt_end + 2 + end + 1))
        });
        let Some((alt, address, used)) = found else {
            out += &rest[..start + 2];
            rest = after;
            continue;
        };
        out += &rest[..start];
        if let Some(address) = target(address) {
            out += &format!("![{alt}]({address})");
        }
        rest = &after[used..];
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses(markdown: &str) -> Vec<String> {
        let mut found = Vec::new();
        rewrite(markdown, |a| {
            found.push(a.to_owned());
            Some(a.to_owned())
        });
        found
    }

    #[test]
    fn a_reference_is_one_line_with_a_safe_address() {
        assert_eq!(
            reference("a [big]\n fox", "https://x.test/a b(1).png"),
            "![a big fox](https://x.test/a%20b%281%29.png)"
        );
    }

    #[test]
    fn references_are_found_outside_code_only() {
        let text = "![a](https://x/1.png) and ![b](https://x/2.png)\n```\n![c](https://x/3.png)\n```\n[d](https://x/4)\n";
        assert_eq!(addresses(text), ["https://x/1.png", "https://x/2.png"]);
    }

    #[test]
    fn references_are_pointed_elsewhere_or_dropped() {
        let text = "![a](https://x/1.png)\n\nText ![b](https://x/2.png) end\n\n> ![c](https://x/1.png)\n\nlast\n";
        let out = rewrite(text, |a| (a == "https://x/1.png").then(|| "k.png".into()));
        assert_eq!(out, "![a](k.png)\n\nText  end\n\n> ![c](k.png)\n\nlast\n");
        let none = rewrite(text, |_| None);
        assert_eq!(none, "Text  end\n\nlast\n");
    }

    #[test]
    fn broken_references_stay_as_text() {
        let text = "![a](https://x/1.png\n![b] (x)\n![](ok.png) ![\n";
        assert_eq!(addresses(text), ["ok.png"]);
        assert_eq!(rewrite(text, |a| Some(a.to_owned())), text);
    }
}
