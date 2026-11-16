//! Queries (Tasks glossary): what a Filter matches Tasks by, on their own Tags.
//!
//! ```text
//! or   := and ('|' and)*
//! and  := not ('&' not)*
//! not  := '!' not | atom
//! atom := '@' tag | '(' or ')'
//! ```
//!
//! Whitespace between terms is ignored. A Tag matches exactly: `@work` does
//! not match `work/acme`. The atom rule is the one place new terms would go.

use pkm_base::tag;
use std::fmt;

/// Characters of a Query that aren't part of a Tag.
const SYNTAX: &str = "@&|!() ";

/// Parentheses nest no deeper than this, so that no text can exhaust the stack.
const MAX_DEPTH: usize = 64;

/// Why a text is no Query, and where: a position in characters, `text.len()`
/// being the end of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub position: usize,
    pub message: String,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.position)
    }
}

impl std::error::Error for QueryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Tag(String),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

/// A parsed Query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query(Expr);

impl Query {
    /// Reads `text`, or says what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, QueryError> {
        let mut parser = Parser {
            chars: text.chars().collect(),
            at: 0,
        };
        let expr = parser.or(0)?;
        parser.skip_space();
        match parser.peek() {
            None => Ok(Self(expr)),
            Some(')') => Err(parser.error("Unexpected )")),
            Some(_) => Err(parser.error("Expected & or |")),
        }
    }

    /// The Tags the Query names, each once, in order of appearance.
    pub fn tags(&self) -> Vec<String> {
        let mut named = Vec::new();
        self.0.collect_tags(&mut named);
        named
    }

    /// Whether a Task carrying exactly `tags` matches.
    pub fn matches(&self, tags: &[String]) -> bool {
        self.0.matches(tags)
    }
}

/// `text` as it may be typed: uppercase letters made lowercase, and everything
/// refused that is neither a Tag character nor an operator, parenthesis or
/// space. What is left may still not be a Query.
pub fn typed(text: &str) -> String {
    let mut kept = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if SYNTAX.contains(c) {
            kept.push(c);
        } else {
            kept.push_str(&tag::strip_forbidden(c.encode_utf8(&mut [0; 4])));
        }
    }
    kept
}

/// The Tag being typed at the end of `text`: what follows its last `@`, if
/// that `@` isn't followed by a space or operator. Empty right after the `@`.
pub fn unfinished_tag(text: &str) -> Option<&str> {
    let word = text.rsplit_once('@')?.1;
    (!word.contains(|c: char| SYNTAX.contains(c))).then_some(word)
}

impl Expr {
    fn collect_tags(&self, named: &mut Vec<String>) {
        match self {
            Self::Tag(tag) => {
                if !named.contains(tag) {
                    named.push(tag.clone());
                }
            }
            Self::Not(inner) => inner.collect_tags(named),
            Self::And(parts) | Self::Or(parts) => {
                parts.iter().for_each(|p| p.collect_tags(named));
            }
        }
    }

    fn matches(&self, tags: &[String]) -> bool {
        match self {
            Self::Tag(tag) => tags.contains(tag),
            Self::Not(inner) => !inner.matches(tags),
            Self::And(parts) => parts.iter().all(|p| p.matches(tags)),
            Self::Or(parts) => parts.iter().any(|p| p.matches(tags)),
        }
    }
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn error(&self, message: &str) -> QueryError {
        self.error_at(self.at, message)
    }

    fn error_at(&self, position: usize, message: &str) -> QueryError {
        QueryError {
            position,
            message: message.to_owned(),
        }
    }

    /// `and ('|' and)*`; `depth` counts the open parentheses.
    fn or(&mut self, depth: usize) -> Result<Expr, QueryError> {
        self.list('|', depth, Self::and, Expr::Or)
    }

    fn and(&mut self, depth: usize) -> Result<Expr, QueryError> {
        self.list('&', depth, Self::not, Expr::And)
    }

    /// `item (sep item)*`, as one `item` if there is no `sep`.
    fn list(
        &mut self,
        sep: char,
        depth: usize,
        item: fn(&mut Self, usize) -> Result<Expr, QueryError>,
        join: fn(Vec<Expr>) -> Expr,
    ) -> Result<Expr, QueryError> {
        let mut parts = vec![item(self, depth)?];
        loop {
            self.skip_space();
            if self.peek() != Some(sep) {
                break;
            }
            self.at += 1;
            parts.push(item(self, depth)?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            join(parts)
        })
    }

    /// `'!'* atom`.
    fn not(&mut self, depth: usize) -> Result<Expr, QueryError> {
        let mut negations = 0;
        loop {
            self.skip_space();
            if self.peek() != Some('!') {
                break;
            }
            self.at += 1;
            negations += 1;
        }
        let expr = self.atom(depth)?;
        // Pairs cancel, so a long run of `!` builds no deep tree.
        Ok(if negations % 2 == 1 {
            Expr::Not(Box::new(expr))
        } else {
            expr
        })
    }

    fn atom(&mut self, depth: usize) -> Result<Expr, QueryError> {
        self.skip_space();
        match self.peek() {
            Some('@') => self.tag(),
            Some('(') => {
                let open = self.at;
                if depth == MAX_DEPTH {
                    return Err(self.error("Too many nested ("));
                }
                self.at += 1;
                let inner = self.or(depth + 1)?;
                self.skip_space();
                if self.peek() != Some(')') {
                    return Err(self.error_at(open, "Unclosed ("));
                }
                self.at += 1;
                Ok(inner)
            }
            _ => Err(self.error("Expected @tag, ! or (")),
        }
    }

    /// `@` and the Tag after it: everything up to a space or an operator.
    fn tag(&mut self) -> Result<Expr, QueryError> {
        let at = self.at;
        self.at += 1;
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && !SYNTAX.contains(c))
        {
            self.at += 1;
        }
        let name: String = self.chars[start..self.at].iter().collect();
        if name.is_empty() {
            return Err(self.error_at(at, "Expected a Tag after @"));
        }
        if !tag::is_valid(&name) {
            return Err(self.error_at(at, "Not a valid Tag"));
        }
        Ok(Expr::Tag(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(query: &str, tags: &[&str]) -> bool {
        let tags: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
        Query::parse(query).unwrap().matches(&tags)
    }

    fn error(text: &str) -> (usize, String) {
        let e = Query::parse(text).unwrap_err();
        (e.position, e.message)
    }

    #[test]
    fn a_query_lists_the_tags_it_names_once() {
        let query = Query::parse("@work & !(@home | @work) | @a/b").unwrap();
        assert_eq!(query.tags(), ["work", "home", "a/b"]);
    }

    #[test]
    fn typing_lowercases_and_refuses_what_a_tag_cannot_hold() {
        assert_eq!(
            typed("@Work & !(@Home_1 | @a/b-2)"),
            "@work & !(@home1 | @a/b-2)"
        );
        assert_eq!(typed("@a ä\n#"), "@a ");
    }

    #[test]
    fn the_unfinished_tag_is_the_word_after_the_last_at() {
        assert_eq!(unfinished_tag("@wo"), Some("wo"));
        assert_eq!(unfinished_tag("@a & !@"), Some(""));
        assert_eq!(unfinished_tag("@a &"), None);
        assert_eq!(unfinished_tag("@a "), None);
        assert_eq!(unfinished_tag("(@a)"), None);
        assert_eq!(unfinished_tag(""), None);
    }

    #[test]
    fn a_tag_matches_tasks_carrying_it() {
        assert!(matches("@work", &["home", "work"]));
        assert!(!matches("@work", &["home"]));
        assert!(!matches("@work", &[]));
    }

    #[test]
    fn a_tag_matches_exactly_not_its_nested_tags() {
        assert!(!matches("@work", &["work/acme"]));
        assert!(!matches("@work/acme", &["work"]));
        assert!(matches("@work/acme", &["work/acme"]));
    }

    #[test]
    fn not_and_or_combine_tags() {
        assert!(matches("@a & @b", &["a", "b"]));
        assert!(!matches("@a & @b", &["a"]));
        assert!(matches("@a | @b", &["b"]));
        assert!(!matches("@a | @b", &["c"]));
        assert!(matches("!@a", &["b"]));
        assert!(!matches("!@a", &["a"]));
        assert!(matches("@work & !@in-review", &["work"]));
        assert!(!matches("@work & !@in-review", &["work", "in-review"]));
    }

    #[test]
    fn not_binds_tighter_than_and_which_binds_tighter_than_or() {
        // @a | (@b & !@c)
        assert!(matches("@a | @b & !@c", &["a", "c"]));
        assert!(matches("@a | @b & !@c", &["b"]));
        assert!(!matches("@a | @b & !@c", &["b", "c"]));
        // (@a | @b) & !@c differs only by the parentheses
        assert!(!matches("(@a | @b) & !@c", &["a", "c"]));
        assert!(matches("(@a | @b) & !@c", &["a"]));
    }

    #[test]
    fn parentheses_group_and_negate() {
        assert!(matches("!(@a | @b)", &["c"]));
        assert!(!matches("!(@a | @b)", &["b"]));
        assert!(matches("((@a))", &["a"]));
        assert!(matches("!!@a", &["a"]));
        assert!(!matches("!!!@a", &["a"]));
    }

    #[test]
    fn whitespace_is_optional() {
        for text in ["@a&@b|!@c", " @a & @b | ! @c ", "@a\t&\n@b|!@c"] {
            assert!(matches(text, &["a", "b"]), "{text:?}");
            assert!(matches(text, &["x"]), "{text:?}");
            assert!(!matches(text, &["a", "c"]), "{text:?}");
        }
    }

    #[test]
    fn a_dangling_operator_is_an_error_where_a_term_was_expected() {
        assert_eq!(error("@a &"), (4, "Expected @tag, ! or (".into()));
        assert_eq!(error("| @a"), (0, "Expected @tag, ! or (".into()));
        assert_eq!(error("@a | | @b"), (5, "Expected @tag, ! or (".into()));
        assert_eq!(error("!"), (1, "Expected @tag, ! or (".into()));
        assert_eq!(error(""), (0, "Expected @tag, ! or (".into()));
    }

    #[test]
    fn an_unclosed_parenthesis_is_an_error_at_its_opening() {
        assert_eq!(error("(@a & @b"), (0, "Unclosed (".into()));
        assert_eq!(error("@a | (@b"), (5, "Unclosed (".into()));
    }

    #[test]
    fn a_stray_closing_parenthesis_or_missing_operator_is_an_error() {
        assert_eq!(error("@a)"), (2, "Unexpected )".into()));
        assert_eq!(error("@a @b"), (3, "Expected & or |".into()));
        assert_eq!(error("()"), (1, "Expected @tag, ! or (".into()));
    }

    #[test]
    fn an_invalid_tag_is_an_error_at_its_at_sign() {
        assert_eq!(error("@ a"), (0, "Expected a Tag after @".into()));
        assert_eq!(error("@a & @Work"), (5, "Not a valid Tag".into()));
        assert_eq!(error("@a_b"), (0, "Not a valid Tag".into()));
        assert_eq!(error("@/a"), (0, "Not a valid Tag".into()));
        assert_eq!(error("work"), (0, "Expected @tag, ! or (".into()));
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        assert_eq!(error("@ä &"), (0, "Not a valid Tag".into()));
        assert_eq!(error("@a & ä"), (5, "Expected @tag, ! or (".into()));
    }

    #[test]
    fn absurd_nesting_is_refused_not_crashed_on() {
        let deep = format!("{}@a{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(error(&deep).1, "Too many nested (");
        assert!(Query::parse(&format!("{}@a", "!".repeat(100_000))).is_ok());
    }
}
