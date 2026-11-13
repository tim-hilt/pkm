//! Tags (App glossary): lowercase names that group Items across Contexts.
//! `/` is reserved for nesting Tags later, so it can't lead, trail or double.

/// Whether `c` may appear in a Tag: `a-z`, `0-9`, `-` or `/`.
fn is_allowed(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '-' | '/')
}

/// Whether `tag` is a valid Tag. There is no length limit.
pub fn is_valid(tag: &str) -> bool {
    !tag.is_empty()
        && tag.chars().all(is_allowed)
        && !tag.starts_with('/')
        && !tag.ends_with('/')
        && !tag.contains("//")
}

/// `text` without the characters a Tag can never contain, for refusing them
/// as they are typed. What is left may still be invalid (`is_valid`), e.g. a
/// lone `/`, since the text may be unfinished.
pub fn strip_forbidden(text: &str) -> String {
    text.chars().filter(|&c| is_allowed(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tags_use_only_lowercase_digits_dash_and_slash() {
        for tag in ["a", "home", "q3-2026", "work/project-x", "0", "-", "a-/-b"] {
            assert!(is_valid(tag), "{tag}");
        }
    }

    #[test]
    fn forbidden_characters_are_invalid() {
        for tag in ["Home", "two words", "a_b", "a.b", "ä", "a\n", "#tag", " a"] {
            assert!(!is_valid(tag), "{tag:?}");
        }
    }

    #[test]
    fn empty_and_misplaced_slashes_are_invalid() {
        for tag in ["", "/", "/a", "a/", "a//b", "//"] {
            assert!(!is_valid(tag), "{tag:?}");
        }
    }

    #[test]
    fn there_is_no_length_limit() {
        assert!(is_valid(&"a".repeat(10_000)));
    }

    #[test]
    fn stripping_removes_only_forbidden_characters() {
        assert_eq!(strip_forbidden("Work Project_1/x-y!"), "orkroject1/x-y");
        assert_eq!(strip_forbidden("ok/"), "ok/");
    }
}
