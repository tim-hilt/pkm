//! The core's own JSON: a value model, a strict parser and a serializer whose
//! output is byte-identical to JavaScript's `JSON.stringify` (ADR-0009).
//!
//! Round-tripping through this module is what lets `.excalidraw` files written
//! by excalidraw.com come back unchanged: object keys keep their order, numbers
//! use JavaScript's formatting, and unknown properties are simply values that
//! nobody interprets.
//!
//! ```
//! use workingtitle_core::json::parse;
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

use std::fmt::{self, Write as _};

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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Object {
    members: Vec<(String, Value)>,
}

impl Object {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Sets `key`. An existing key keeps its position; a new key goes last.
    pub fn insert(&mut self, key: String, value: Value) {
        match self.members.iter_mut().find(|(k, _)| *k == key) {
            Some((_, existing)) => *existing = value,
            None => self.members.push((key, value)),
        }
    }

    /// Members in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Value)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
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
    let mut parser = Parser { input, pos: 0 };
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
            object.insert(key, self.parse_value(depth)?);
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
        Value::Object(object) => {
            let members: Vec<_> = object.iter().collect();
            write_container(out, '{', '}', members.len(), indent, level, |out, i| {
                let (key, member) = members[i];
                write_string(out, key);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, member, indent, level + 1);
            });
        }
    }
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
