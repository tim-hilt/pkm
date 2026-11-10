//! The app's own JSON: a value model, a strict parser and a serializer whose
//! output is byte-identical to JavaScript's `JSON.stringify` (ADR-0009).
//!
//! Round-tripping through this module is what lets `.excalidraw` files written
//! by excalidraw.com come back unchanged: object keys keep their order, numbers
//! use JavaScript's formatting, and unknown properties are simply values that
//! nobody interprets.
//!
//! ```
//! use pkm_base::json::parse;
//!
//! let value = parse(r#"{ "b": 1.0, "a": [true, null] }"#).unwrap();
//! assert_eq!(value.to_json(), r#"{"b":1,"a":[true,null]}"#);
//! ```
//!
//! Deliberate behaviour, all matching `JSON.parse` / `JSON.stringify` unless
//! noted:
//! - Numbers are `f64`. Integers beyond 2^53 lose precision; numbers outside
//!   the `f64` range parse to infinity and serialize as `null`.
//! - A duplicate object key keeps its first position and takes the last value.
//! - Syntax is strict: no comments, trailing commas, BOM or single quotes.
//! - Unlike JavaScript, an unpaired surrogate escape (`"\ud800"`) is rejected,
//!   because Rust strings cannot represent it.
//! - Nesting deeper than [`MAX_DEPTH`] is rejected so hostile files cannot
//!   overflow the stack.

use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::sync::Arc;

/// Deepest array/object nesting [`parse`] accepts.
pub const MAX_DEPTH: usize = 512;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Object),
}

impl Value {
    /// The member `key` if this is an object that has it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The number if it is a whole one `f64` holds exactly (0 to 2^53).
    pub fn as_u64(&self) -> Option<u64> {
        let n = self.as_f64()?;
        (n.fract() == 0.0 && (0.0..=9_007_199_254_740_992.0).contains(&n)).then_some(n as u64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Compact serialization, like `JSON.stringify(value)`.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self, None, 0);
        out
    }

    /// Two-space indented serialization, like `JSON.stringify(value, null, 2)`
    /// — the layout excalidraw.com saves `.excalidraw` files in.
    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self, Some("  "), 0);
        out
    }
}

/// A JSON object: members in insertion order, keys unique.
///
/// Lookup is a linear scan. Excalidraw objects are small (an Element has
/// about thirty members), where that beats hashing and keeps order for free.
///
/// Keys are shared: a parsed document keeps one copy of each distinct key
/// (`x`, `strokeColor`, ...) however many objects use it, so that an Element
/// is cheap to copy and to keep.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Object {
    members: Vec<(Arc<str>, Value)>,
}

impl Object {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.members
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.members
            .iter_mut()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v)
    }

    /// Sets `key`. An existing key keeps its position; a new key goes last.
    pub fn insert(&mut self, key: String, value: Value) {
        match self.members.iter_mut().find(|(k, _)| **k == *key) {
            Some((_, existing)) => *existing = value,
            None => self.members.push((key.into(), value)),
        }
    }

    /// [`Object::insert`] for a key that is already shared.
    fn insert_shared(&mut self, key: Arc<str>, value: Value) {
        match self.members.iter_mut().find(|(k, _)| *k == key) {
            Some((_, existing)) => *existing = value,
            None => self.members.push((key, value)),
        }
    }

    /// Removes `key`, if present.
    pub fn remove(&mut self, key: &str) {
        self.members.retain(|(k, _)| &**k != key);
    }

    /// Members in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Value)> {
        self.members.iter().map(|(k, v)| (&**k, v))
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Like [`Value::to_json_pretty`], without having to wrap (and so clone)
    /// the object in a [`Value`].
    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        write_object(&mut out, self, Some("  "), 0);
        out
    }
}

// --- Errors ---------------------------------------------------------------

/// Why a document was rejected, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// What is wrong, e.g. `expected ',' or ']', found '}'`.
    pub message: String,
    /// Byte offset into the input of the offending character (the input's
    /// length for an unexpected end).
    pub offset: usize,
    /// 1-based line of `offset`.
    pub line: usize,
    /// 1-based column of `offset`, counted in characters.
    pub column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid JSON at line {}, column {} (byte {}): {}",
            self.line, self.column, self.offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

// --- Parsing --------------------------------------------------------------

/// Parses one complete JSON document.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let mut parser = Parser {
        input,
        pos: 0,
        keys: HashMap::new(),
    };
    parser.skip_whitespace();
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.pos < input.len() {
        return Err(parser.unexpected("end of document"));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    /// Every distinct key met so far, to share it between objects.
    keys: HashMap<String, Arc<str>>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// Error at an explicit offset, with line and column derived from it.
    fn error_at(&self, offset: usize, message: impl Into<String>) -> ParseError {
        let before = &self.input[..offset];
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        ParseError {
            message: message.into(),
            offset,
            line,
            column: before[line_start..].chars().count() + 1,
        }
    }

    /// "expected X, found Y" at the current position.
    fn unexpected(&self, expected: &str) -> ParseError {
        let found = match self.input[self.pos..].chars().next() {
            Some(c) => format!("{c:?}"),
            None => "end of input".to_string(),
        };
        self.error_at(self.pos, format!("expected {expected}, found {found}"))
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.unexpected(&format!("{:?}", byte as char)))
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<Value, ParseError> {
        match self.peek() {
            Some(b'{') => self.parse_object(depth),
            Some(b'[') => self.parse_array(depth),
            Some(b'"') => self.parse_string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(b't') => self.parse_literal("true", Value::Bool(true)),
            Some(b'f') => self.parse_literal("false", Value::Bool(false)),
            Some(b'n') => self.parse_literal("null", Value::Null),
            _ => Err(self.unexpected("a value")),
        }
    }

    fn enter(&self, depth: usize) -> Result<usize, ParseError> {
        if depth >= MAX_DEPTH {
            Err(self.error_at(self.pos, format!("nesting deeper than {MAX_DEPTH} levels")))
        } else {
            Ok(depth + 1)
        }
    }

    fn parse_literal(&mut self, word: &str, value: Value) -> Result<Value, ParseError> {
        if self.input[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error_at(self.pos, format!("expected '{word}'")))
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Value, ParseError> {
        let depth = self.enter(depth)?;
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.parse_value(depth)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.unexpected("',' or ']'")),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Value, ParseError> {
        let depth = self.enter(depth)?;
        self.pos += 1; // '{'
        let mut object = Object::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(object));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.unexpected("a string key"));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let key = match self.keys.get(&key) {
                Some(shared) => Arc::clone(shared),
                None => {
                    let shared: Arc<str> = Arc::from(key.as_str());
                    self.keys.insert(key, Arc::clone(&shared));
                    shared
                }
            };
            object.insert_shared(key, self.parse_value(depth)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(object));
                }
                _ => return Err(self.unexpected("',' or '}'")),
            }
        }
    }

    /// Strict JSON number grammar: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.skip_digits(),
            _ => return Err(self.unexpected("a digit")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.expect_digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.expect_digits()?;
        }
        // The grammar above guarantees Rust can parse it; out-of-range
        // magnitudes become infinity, as in JavaScript.
        let number = self.input[start..self.pos]
            .parse::<f64>()
            .map_err(|e| self.error_at(start, e.to_string()))?;
        Ok(Value::Number(number))
    }

    fn skip_digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }

    fn expect_digits(&mut self) -> Result<(), ParseError> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(self.unexpected("a digit"));
        }
        self.skip_digits();
        Ok(())
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            // Copy the run of ordinary characters in one go. Stopping only at
            // ASCII bytes keeps `run_start..self.pos` on character boundaries.
            let run_start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(&self.input[run_start..self.pos]);

            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => self.parse_escape(&mut out)?,
                Some(_) => {
                    return Err(
                        self.error_at(self.pos, "control character in string must be escaped")
                    );
                }
                None => return Err(self.error_at(self.pos, "unterminated string")),
            }
        }
    }

    /// Decodes one escape sequence starting at the backslash.
    fn parse_escape(&mut self, out: &mut String) -> Result<(), ParseError> {
        let escape_start = self.pos;
        self.pos += 1; // backslash
        let decoded = match self.peek() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.pos += 1;
                out.push(self.parse_unicode_escape(escape_start)?);
                return Ok(());
            }
            Some(_) => return Err(self.unexpected("a valid escape character")),
            None => return Err(self.error_at(self.pos, "unterminated string")),
        };
        self.pos += 1;
        out.push(decoded);
        Ok(())
    }

    /// Decodes the `XXXX` of `\uXXXX` (cursor just after the `u`), combining
    /// a high surrogate with the `\uXXXX` that must follow it.
    fn parse_unicode_escape(&mut self, escape_start: usize) -> Result<char, ParseError> {
        let unit = self.parse_hex4()?;
        let code = match unit {
            0xD800..=0xDBFF => {
                if !self.input[self.pos..].starts_with("\\u") {
                    return Err(self.unpaired_surrogate(escape_start));
                }
                self.pos += 2;
                let low = self.parse_hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(self.unpaired_surrogate(escape_start));
                }
                0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.unpaired_surrogate(escape_start)),
            _ => unit,
        };
        // Surrogates are excluded above, so this is always a valid scalar.
        Ok(char::from_u32(code).expect("valid scalar value"))
    }

    fn unpaired_surrogate(&self, escape_start: usize) -> ParseError {
        self.error_at(escape_start, "unpaired surrogate in \\u escape")
    }

    fn parse_hex4(&mut self) -> Result<u32, ParseError> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(b) => (b as char).to_digit(16),
                None => return Err(self.error_at(self.pos, "unterminated string")),
            };
            let Some(digit) = digit else {
                return Err(self.unexpected("a hex digit"));
            };
            value = value * 16 + digit;
            self.pos += 1;
        }
        Ok(value)
    }
}

// --- Serializing ----------------------------------------------------------

/// `indent` is the per-level indentation, or `None` for compact output.
fn write_value(out: &mut String, value: &Value, indent: Option<&str>, level: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(out, *n),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            write_container(out, '[', ']', items.len(), indent, level, |out, i| {
                write_value(out, &items[i], indent, level + 1);
            })
        }
        Value::Object(object) => write_object(out, object, indent, level),
    }
}

fn write_object(out: &mut String, object: &Object, indent: Option<&str>, level: usize) {
    let members = &object.members;
    write_container(out, '{', '}', members.len(), indent, level, |out, i| {
        let (key, member) = &members[i];
        write_string(out, key);
        out.push(':');
        if indent.is_some() {
            out.push(' ');
        }
        write_value(out, member, indent, level + 1);
    });
}

/// Shared layout of arrays and objects: delimiters, commas and line breaks.
/// `write_item` writes the `i`th of `len` items.
fn write_container(
    out: &mut String,
    open: char,
    close: char,
    len: usize,
    indent: Option<&str>,
    level: usize,
    mut write_item: impl FnMut(&mut String, usize),
) {
    out.push(open);
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        write_line_break(out, indent, level + 1);
        write_item(out, i);
    }
    if len > 0 {
        write_line_break(out, indent, level);
    }
    out.push(close);
}

fn write_line_break(out: &mut String, indent: Option<&str>, level: usize) {
    if let Some(indent) = indent {
        out.push('\n');
        for _ in 0..level {
            out.push_str(indent);
        }
    }
}

/// Quotes and escapes exactly as `JSON.stringify` does: only `"`, `\` and
/// control characters are escaped; everything else, including `/`, DEL and
/// U+2028, is written raw.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    let mut run_start = 0;
    for (i, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x08 => "\\b",
            0x0c => "\\f",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0..0x20 => "",
            _ => continue,
        };
        out.push_str(&s[run_start..i]);
        if escape.is_empty() {
            let _ = write!(out, "\\u{b:04x}");
        } else {
            out.push_str(escape);
        }
        run_start = i + 1;
    }
    out.push_str(&s[run_start..]);
    out.push('"');
}

/// Formats like ECMAScript's `Number::toString`, with non-finite values as
/// `null` (what `JSON.stringify` writes for them).
fn write_number(out: &mut String, n: f64) {
    if !n.is_finite() {
        out.push_str("null");
        return;
    }
    // Fast path for the overwhelmingly common whole numbers (also folds -0
    // into 0, as JavaScript does).
    if n.fract() == 0.0 && n.abs() < 1e15 {
        let _ = write!(out, "{}", n as i64);
        return;
    }
    if n < 0.0 {
        out.push('-');
    }
    let (digits, exponent) = shortest_digits(n.abs());
    let k = digits.len() as i32;
    let n = exponent + 1; // value = 0.DIGITS × 10^n

    if k <= n && n <= 21 {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-n) as usize));
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(
            out,
            "e{}{}",
            if n - 1 < 0 { '-' } else { '+' },
            (n - 1).abs()
        );
    }
}

/// The shortest decimal digits that round-trip to `n` (finite, positive) and
/// the decimal exponent of the first digit: `n = d.ddd… × 10^exponent`.
///
/// Rust and JavaScript agree on the digit count, but when two shortest
/// candidates are exactly equally close to `n`, Rust rounds up and JavaScript
/// picks the even digit (14611050304372.0625 is `…372.062` in JS). Such ties
/// need an exact expansion of 16+ digits, so shorter results skip the check.
fn shortest_digits(n: f64) -> (String, i32) {
    let (mut digits, exponent) = scientific_digits(&format!("{n:e}"));
    let k = digits.len();
    let last = digits.as_bytes()[k - 1];
    if k >= 15 && last % 2 == 1 {
        // A tie means n is exactly `<digits with last-1>5`: print 30 more
        // digits than needed and see whether that is all there is.
        let (exact, exact_exponent) = scientific_digits(&format!("{n:.*e}", k + 30));
        let (head, tail) = exact.split_at(k + 1);
        let even_neighbour = format!("{}{}", &digits[..k - 1], (last - 1) as char);
        if exact_exponent == exponent
            && head == format!("{even_neighbour}5")
            && tail.bytes().all(|b| b == b'0')
        {
            digits = even_neighbour;
        }
    }
    (digits, exponent)
}

/// Splits Rust's `{:e}` output ("1.2345e-7") into its digits and exponent.
fn scientific_digits(formatted: &str) -> (String, i32) {
    let (mantissa, exponent) = formatted.split_once('e').expect("`{:e}` has an exponent");
    let digits = mantissa.chars().filter(|&c| c != '.').collect();
    (
        digits,
        exponent.parse().expect("`{:e}` exponent is an integer"),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour through the public interface only: `parse`, `Value::to_json`
    //! and `Value::to_json_pretty`. Expected strings are literals taken from
    //! JavaScript's `JSON.stringify`, never recomputed by the code.

    use super::{Value, parse};

    #[test]
    fn as_u64_takes_only_whole_numbers_that_are_exact() {
        let u64_of = |text: &str| parse(text).unwrap().as_u64();
        assert_eq!(u64_of("0"), Some(0));
        assert_eq!(u64_of("1700000000000"), Some(1_700_000_000_000));
        assert_eq!(u64_of("2.0"), Some(2));
        assert_eq!(u64_of("9007199254740992"), Some(1 << 53));
        for text in ["1.5", "-1", "1e300", "\"1\"", "null"] {
            assert_eq!(u64_of(text), None, "{text}");
        }
    }

    /// Parses `input` and serializes it compactly again.
    fn round_trip(input: &str) -> String {
        parse(input).expect("valid JSON").to_json()
    }

    // --- Key order ------------------------------------------------------------

    #[test]
    fn object_keys_keep_insertion_order() {
        let input = r#"{"z":1,"a":2,"m":{"y":true,"b":false},"10":3,"2":4}"#;
        assert_eq!(round_trip(input), input);
    }

    #[test]
    fn duplicate_key_keeps_first_position_and_last_value_like_javascript() {
        // JSON.stringify(JSON.parse('{"a":1,"b":2,"a":3}')) === '{"a":3,"b":2}'
        assert_eq!(round_trip(r#"{"a":1,"b":2,"a":3}"#), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn object_lookup_finds_values_by_key() {
        let value = parse(r#"{"type":"rectangle","x":1.5,"locked":false,"tags":[null]}"#).unwrap();
        assert_eq!(value.get("type").and_then(Value::as_str), Some("rectangle"));
        assert_eq!(value.get("x").and_then(Value::as_f64), Some(1.5));
        assert_eq!(value.get("locked").and_then(Value::as_bool), Some(false));
        assert_eq!(
            value
                .get("tags")
                .and_then(Value::as_array)
                .map(<[Value]>::len),
            Some(1)
        );
        assert_eq!(value.get("missing"), None);
    }

    // --- Numbers --------------------------------------------------------------

    #[test]
    fn numbers_serialize_like_javascript() {
        // (input, JSON.stringify(JSON.parse(input)))
        let cases = [
            ("0", "0"),
            ("-0", "0"),
            ("1.0", "1"),
            ("100", "100"),
            ("1E2", "100"),
            ("-12.50", "-12.5"),
            ("4.35", "4.35"),
            ("0.30000000000000004", "0.30000000000000004"),
            ("0.1", "0.1"),
            ("123.45000000000002", "123.45000000000002"),
            ("0.000001", "0.000001"),
            ("0.0000001", "1e-7"),
            ("1.5e-7", "1.5e-7"),
            ("-1.5e-10", "-1.5e-10"),
            ("100000000000000000000", "100000000000000000000"),
            ("1e20", "100000000000000000000"),
            ("1e21", "1e+21"),
            ("1.5e+25", "1.5e+25"),
            ("-1e300", "-1e+300"),
            ("123456789012345678901", "123456789012345680000"),
            ("9007199254740993", "9007199254740992"),
            ("5e-324", "5e-324"),
            ("1.7976931348623157e308", "1.7976931348623157e+308"),
            ("1730380000000", "1730380000000"),
        ];
        for (input, expected) in cases {
            assert_eq!(round_trip(input), expected, "number {input}");
        }
    }

    #[test]
    fn exact_ties_between_shortest_candidates_pick_the_even_digit_like_javascript() {
        // Both candidates are equally close; Rust alone would round up.
        for (input, expected) in [
            ("14611050304372.0625", "14611050304372.062"),
            ("-2045733348428895.25", "-2045733348428895.2"),
            ("1000000000.00390625", "1000000000.0039062"),
        ] {
            assert_eq!(round_trip(input), expected, "number {input}");
        }
    }

    #[test]
    fn numbers_out_of_f64_range_become_null_like_javascript() {
        // JSON.stringify(JSON.parse("1e999")) === "null" (Infinity has no JSON form)
        assert_eq!(round_trip("[1e999,-1e999]"), "[null,null]");
    }

    // --- Strings --------------------------------------------------------------

    #[test]
    fn all_simple_escapes_are_decoded() {
        let value = parse(r#""\" \\ \/ \b \f \n \r \t""#).unwrap();
        assert_eq!(value.as_str(), Some("\" \\ / \u{8} \u{c} \n \r \t"));
    }

    #[test]
    fn unicode_escapes_are_decoded() {
        assert_eq!(
            parse(r#""\u0041\u00e9\u65E5""#).unwrap().as_str(),
            Some("Aé日")
        );
    }

    #[test]
    fn surrogate_pair_escapes_combine_into_one_character() {
        assert_eq!(parse(r#""\ud83d\ude00""#).unwrap().as_str(), Some("😀"));
        assert_eq!(
            parse(r#""\uD834\uDD1E""#).unwrap().as_str(),
            Some("\u{1d11e}")
        );
    }

    #[test]
    fn raw_unicode_passes_through_unescaped() {
        let input = "\"Héllo 日本語 😀 \u{1d11e} \u{2028}\"";
        assert_eq!(round_trip(input), input);
    }

    #[test]
    fn serializer_escapes_like_json_stringify() {
        // JSON.stringify("\"\\/\b\f\n\r\t\u0001\u001f\u007f") — "/" and DEL stay raw.
        let value = parse(r#""\"\\\/\b\f\n\r\t\u0001\u001F\u007f""#).unwrap();
        assert_eq!(
            value.to_json(),
            "\"\\\"\\\\/\\b\\f\\n\\r\\t\\u0001\\u001f\u{7f}\""
        );
    }

    #[test]
    fn object_keys_are_escaped_like_strings() {
        let input = r#"{"a\"b\n":1,"日本":2}"#;
        assert_eq!(round_trip(input), input);
    }

    // --- Whole values & formatting -------------------------------------------

    #[test]
    fn every_value_kind_round_trips() {
        let input =
            r#"{"n":null,"t":true,"f":false,"i":-3,"s":"x","a":[1,[2,[]],{}],"o":{"k":{}}}"#;
        assert_eq!(round_trip(input), input);
    }

    #[test]
    fn insignificant_whitespace_is_dropped_in_compact_output() {
        assert_eq!(
            round_trip(" \t\r\n{ \"a\" :\n[ 1 , 2 ] }\n"),
            r#"{"a":[1,2]}"#
        );
    }

    #[test]
    fn pretty_output_matches_json_stringify_with_indent_2() {
        // JSON.stringify({a:[1,{b:null}],c:{},d:[],e:"x"}, null, 2)
        let expected = "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": {},\n  \"d\": [],\n  \"e\": \"x\"\n}";
        let value = parse(r#"{"a":[1,{"b":null}],"c":{},"d":[],"e":"x"}"#).unwrap();
        assert_eq!(value.to_json_pretty(), expected);
    }

    #[test]
    fn scalar_and_empty_documents_round_trip() {
        for input in ["null", "true", "0", r#""s""#, "[]", "{}"] {
            assert_eq!(round_trip(input), input);
            assert_eq!(parse(input).unwrap().to_json_pretty(), input);
        }
    }

    // --- Rejection with position ----------------------------------------------

    /// Parses invalid input and returns (byte offset, line, column).
    fn error_position(input: &str) -> (usize, usize, usize) {
        let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
        assert!(!error.message.is_empty());
        (error.offset, error.line, error.column)
    }

    #[test]
    fn error_reports_offset_line_and_column() {
        assert_eq!(error_position(r#"{"a": }"#), (6, 1, 7));
        assert_eq!(error_position("{\n  \"a\": 1,\n  \"b\" 2\n}"), (18, 3, 7));
    }

    #[test]
    fn error_column_counts_characters_not_bytes() {
        // "é" is two bytes but one column.
        assert_eq!(error_position("[\"é\" 1]"), (6, 1, 6));
    }

    #[test]
    fn error_display_names_the_position_and_the_problem() {
        let error = parse(r#"{"a": }"#).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("line 1, column 7"), "{text}");
        assert!(text.contains(&error.message), "{text}");
    }

    #[test]
    fn syntax_errors_are_rejected_at_the_offending_character() {
        let cases: [(&str, usize); 24] = [
            ("", 0),            // empty input
            ("   ", 3),         // only whitespace
            ("[1,]", 3),        // trailing comma in array
            (r#"{"a":1,}"#, 7), // trailing comma in object
            ("[1 2]", 3),       // missing comma
            (r#"{"a" 1}"#, 5),  // missing colon
            (r#"{a:1}"#, 1),    // unquoted key
            ("{'a':1}", 1),     // single quotes
            ("[1] x", 4),       // trailing data
            ("[1", 2),          // unterminated array
            (r#"{"a":1"#, 6),   // unterminated object
            (r#""abc"#, 4),     // unterminated string
            ("\"a\nb\"", 2),    // raw newline in string
            (r#""\x""#, 2),     // unknown escape
            (r#""\u12G4""#, 5), // bad hex digit
            ("01", 1),          // leading zero
            ("+1", 0),          // plus sign
            (".5", 0),          // missing integer part
            ("1.", 2),          // missing fraction digits
            ("1e", 2),          // missing exponent digits
            ("NaN", 0),         // not JSON
            ("tru", 0),         // truncated literal
            ("// c\n1", 0),     // comments
            ("\u{feff}1", 0),   // BOM
        ];
        for (input, offset) in cases {
            let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
            assert_eq!(error.offset, offset, "offset for {input:?}: {error}");
        }
    }

    #[test]
    fn unpaired_surrogate_escapes_are_rejected() {
        // Rust strings cannot hold lone surrogates; reject rather than corrupt.
        for input in [
            r#""\ud83d""#,
            r#""\ude00""#,
            r#""\ud83dx""#,
            r#""\ud83d\u0041""#,
        ] {
            let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
            assert!(error.message.contains("surrogate"), "{error}");
        }
    }

    #[test]
    fn excessive_nesting_is_rejected_instead_of_overflowing_the_stack() {
        let deep = "[".repeat(100_000);
        let error = parse(&deep).unwrap_err();
        assert!(error.message.contains("nesting"), "{error}");
    }

    #[test]
    fn reasonable_nesting_is_accepted() {
        let input = format!("{}{}", "[".repeat(200), "]".repeat(200));
        assert_eq!(round_trip(&input), input);
    }
}
