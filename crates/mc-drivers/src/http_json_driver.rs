//! HTTP/JSON source driver.
//!
//! Wraps `ureq` (sync HTTP client). Performs a single GET request, parses
//! the JSON body with a minimal in-tree parser (see `mod json`), navigates
//! to an array of records via `json_path`, and yields rows.
//!
//! ## JSON parsing
//!
//! Phase 5A intentionally does not pull `serde_json` (not in ADR-0010
//! Decision 4's pin matrix). The in-tree parser supports the full RFC 8259
//! grammar to the depth Mosaic needs:
//!   - scalars: `null`, `true`, `false`, numbers (parsed as `f64`),
//!     strings (with `\\`/`\"`/`\n`/`\t`/`\r`/`\u00XX` escapes);
//!   - arrays and objects (with arbitrary nesting);
//!   - duplicate object keys keep the **last** value (consistent with most
//!     JSON consumers).
//! It does NOT support streaming/incremental parsing; the entire response
//! body is materialised. For Phase 5A reference workloads (HubSpot CSV
//! exports, REST API result pages of ~10K rows) this is fine.
//!
//! ## `json_path` selector
//!
//! `None` → the response body itself must be a JSON array of objects
//! (each object becomes one row).
//!
//! `Some("a.b.c")` → starting at the response root, follow `a` then `b`
//! then `c` into nested objects; the final value must be a JSON array of
//! objects. Numeric path segments are NOT array-indexed in Phase 5A
//! (paginated responses with `[…].data[0].rows` are out of scope; the
//! single dotted-path syntax matches the recipe schema in
//! ADR-0010 Decision 7).
//!
//! ## Schema inference rule
//!
//! After fetching, the driver inspects every row in the array and
//! computes the schema as the **union** of observed scalar types per
//! field name:
//!
//!   - if every observed non-null value is a JSON number → `F64`
//!   - else if every observed non-null value is a JSON boolean → `Bool`
//!   - else if every observed non-null value is a JSON string → `Str`
//!   - else (mixed scalars, or any nested object/array, or no observed
//!     values) → `Str` (objects and arrays are JSON-serialized)
//!
//! `nullable` is `true` whenever any row's value for that field is
//! `null` or the field is absent. The ordering of columns in the
//! returned schema matches **first-seen-key** order across rows, which
//! is deterministic for a given JSON input.

use crate::{
    Column, ColumnData, ColumnDataType, ColumnSchema, DriverError, RowBatch, SourceDriver,
};

/// Construct an HTTP/JSON driver.
///
/// `url` is fetched with `GET` and a 30-second total timeout. `json_path`
/// (e.g., `Some("data.rows")`) navigates into a nested array; pass `None`
/// when the response body itself is the array.
pub fn http_json_driver(url: &str, json_path: Option<&str>) -> Result<HttpJsonDriver, DriverError> {
    HttpJsonDriver::new(url, json_path)
}

/// HTTP/JSON driver. After construction, the inferred schema and an
/// in-memory array of parsed rows are held; `fetch_batch` drains chunks.
#[derive(Debug)]
pub struct HttpJsonDriver {
    #[allow(dead_code)]
    url: String,
    schema: Vec<ColumnSchema>,
    columns: Vec<ColumnBuffer>,
    cursor: usize,
    total_rows: usize,
    cancelled: bool,
}

impl HttpJsonDriver {
    fn new(url: &str, json_path: Option<&str>) -> Result<Self, DriverError> {
        let response = ureq::get(url)
            .timeout(std::time::Duration::from_secs(30))
            .call()
            .map_err(|e| {
                // ureq 2.x `Error` distinguishes Status (HTTP-level) from
                // Transport (TCP/DNS-level). We mirror this into the
                // DriverError variants.
                match e {
                    ureq::Error::Status(code, resp) => DriverError::HttpStatus {
                        url: url.to_string(),
                        status: code,
                        body_preview: resp
                            .into_string()
                            .ok()
                            .map(|s| s.chars().take(200).collect::<String>())
                            .unwrap_or_default(),
                    },
                    ureq::Error::Transport(t) => DriverError::ConnectionFailed {
                        target: url.to_string(),
                        message: t.to_string(),
                    },
                }
            })?;

        let body = response.into_string().map_err(|e| DriverError::Io {
            path: std::path::PathBuf::from(url),
            message: e.to_string(),
        })?;

        let root = json::parse(&body).map_err(|e| DriverError::MalformedSource {
            message: format!("invalid JSON from {}: {}", url, e),
        })?;

        let rows: Vec<json::Value> = {
            let array = navigate(&root, json_path, url)?;
            match array {
                json::Value::Array(items) => items.clone(),
                _ => {
                    return Err(DriverError::JsonPathNotArray {
                        url: url.to_string(),
                        json_path: json_path.unwrap_or("(root)").to_string(),
                    });
                }
            }
        };

        let (schema, columns) = build_columns(&rows);
        let total_rows = columns.first().map(ColumnBuffer::len).unwrap_or(0);

        Ok(HttpJsonDriver {
            url: url.to_string(),
            schema,
            columns,
            cursor: 0,
            total_rows,
            cancelled: false,
        })
    }
}

impl SourceDriver for HttpJsonDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Ok(self.schema.clone())
    }

    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.cancelled || max_rows == 0 || self.cursor >= self.total_rows {
            return Ok(None);
        }
        let end = (self.cursor + max_rows).min(self.total_rows);
        let take = end - self.cursor;
        let columns = self
            .schema
            .iter()
            .zip(self.columns.iter())
            .map(|(s, b)| Column {
                name: s.name.clone(),
                data: b.slice(self.cursor, end),
            })
            .collect();
        self.cursor = end;
        Ok(Some(RowBatch {
            columns,
            row_count: take,
        }))
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.cursor = self.total_rows;
    }
}

fn navigate<'a>(
    root: &'a json::Value,
    path: Option<&str>,
    url: &str,
) -> Result<&'a json::Value, DriverError> {
    let path = match path {
        None => return Ok(root),
        Some(p) => p,
    };
    let mut current = root;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        let map = match current {
            json::Value::Object(m) => m,
            _ => {
                return Err(DriverError::JsonPathNotArray {
                    url: url.to_string(),
                    json_path: path.to_string(),
                });
            }
        };
        current = match map.iter().find(|(k, _)| k == segment) {
            Some((_, v)) => v,
            None => {
                return Err(DriverError::JsonPathNotArray {
                    url: url.to_string(),
                    json_path: path.to_string(),
                });
            }
        };
    }
    Ok(current)
}

#[derive(Debug)]
enum ColumnBuffer {
    F64(Vec<Option<f64>>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}

impl ColumnBuffer {
    fn len(&self) -> usize {
        match self {
            ColumnBuffer::F64(v) => v.len(),
            ColumnBuffer::Str(v) => v.len(),
            ColumnBuffer::Bool(v) => v.len(),
        }
    }

    fn slice(&self, start: usize, end: usize) -> ColumnData {
        match self {
            ColumnBuffer::F64(v) => ColumnData::F64(v[start..end].to_vec()),
            ColumnBuffer::Str(v) => ColumnData::Str(v[start..end].to_vec()),
            ColumnBuffer::Bool(v) => ColumnData::Bool(v[start..end].to_vec()),
        }
    }
}

/// Two-pass: first pass collects field types per first-seen-key order;
/// second pass fills column buffers, padding with `None` for any row
/// missing a field.
fn build_columns(rows: &[json::Value]) -> (Vec<ColumnSchema>, Vec<ColumnBuffer>) {
    // Pass 1: discover field order + accumulate per-field type info.
    let mut order: Vec<String> = Vec::new();
    let mut info: std::collections::HashMap<String, FieldInfo> = std::collections::HashMap::new();

    for row in rows {
        if let json::Value::Object(map) = row {
            for (k, v) in map {
                if !info.contains_key(k) {
                    order.push(k.clone());
                    info.insert(k.clone(), FieldInfo::default());
                }
                if let Some(fi) = info.get_mut(k) {
                    fi.observe(v);
                }
            }
        }
    }
    // Mark fields absent in some rows as nullable.
    for row in rows {
        if let json::Value::Object(map) = row {
            for k in &order {
                if !map.iter().any(|(mk, _)| mk == k) {
                    if let Some(fi) = info.get_mut(k) {
                        fi.has_null = true;
                    }
                }
            }
        }
    }

    let schema: Vec<ColumnSchema> = order
        .iter()
        .map(|name| {
            let fi = info.get(name).cloned().unwrap_or_default();
            ColumnSchema {
                name: name.clone(),
                data_type: fi.resolve_type(),
                nullable: fi.has_null,
            }
        })
        .collect();

    // Pass 2: build column buffers.
    let mut columns: Vec<ColumnBuffer> = schema
        .iter()
        .map(|s| match s.data_type {
            ColumnDataType::F64 => ColumnBuffer::F64(Vec::with_capacity(rows.len())),
            ColumnDataType::Bool => ColumnBuffer::Bool(Vec::with_capacity(rows.len())),
            // Note: I64 isn't generated by JSON inference (numbers always
            // → F64); Str is the catch-all for everything else.
            ColumnDataType::I64 => ColumnBuffer::F64(Vec::with_capacity(rows.len())),
            ColumnDataType::Str => ColumnBuffer::Str(Vec::with_capacity(rows.len())),
        })
        .collect();

    for row in rows {
        let map = match row {
            json::Value::Object(m) => m,
            _ => continue,
        };
        for (i, schema_col) in schema.iter().enumerate() {
            let value = map
                .iter()
                .find(|(k, _)| k == &schema_col.name)
                .map(|(_, v)| v);
            push_into(&mut columns[i], value);
        }
    }

    (schema, columns)
}

fn push_into(buf: &mut ColumnBuffer, value: Option<&json::Value>) {
    match buf {
        ColumnBuffer::F64(v) => match value {
            None | Some(json::Value::Null) => v.push(None),
            Some(json::Value::Number(n)) => v.push(Some(*n)),
            Some(other) => v.push(other.as_f64()),
        },
        ColumnBuffer::Bool(v) => match value {
            None | Some(json::Value::Null) => v.push(None),
            Some(json::Value::Bool(b)) => v.push(Some(*b)),
            Some(_) => v.push(None),
        },
        ColumnBuffer::Str(v) => match value {
            None | Some(json::Value::Null) => v.push(None),
            Some(json::Value::String(s)) => v.push(Some(s.clone())),
            Some(other) => v.push(Some(other.serialize())),
        },
    }
}

#[derive(Default, Clone)]
struct FieldInfo {
    has_null: bool,
    has_number: bool,
    has_bool: bool,
    has_string: bool,
    has_other: bool,
}

impl FieldInfo {
    fn observe(&mut self, v: &json::Value) {
        match v {
            json::Value::Null => self.has_null = true,
            json::Value::Number(_) => self.has_number = true,
            json::Value::Bool(_) => self.has_bool = true,
            json::Value::String(_) => self.has_string = true,
            _ => self.has_other = true,
        }
    }

    fn resolve_type(&self) -> ColumnDataType {
        // "every observed non-null value is X" → X.
        let observed: u8 = (self.has_number as u8)
            + (self.has_bool as u8)
            + (self.has_string as u8)
            + (self.has_other as u8);
        if observed == 0 {
            return ColumnDataType::Str;
        }
        if observed == 1 {
            if self.has_number {
                return ColumnDataType::F64;
            }
            if self.has_bool {
                return ColumnDataType::Bool;
            }
            if self.has_string {
                return ColumnDataType::Str;
            }
        }
        // Mixed scalar types or any object/array → Str (JSON-serialized
        // for objects/arrays at push time).
        ColumnDataType::Str
    }
}

// ============================================================================
// Minimal in-tree JSON parser.
//
// Phase 5A constraint: do not pull serde_json (not in ADR-0010 Decision 4
// matrix). The grammar implemented here is RFC 8259-conformant for
// strings, numbers, booleans, null, arrays, and objects. Numbers are
// always parsed as f64 (Mosaic's only numeric carrier on the ingestion
// path).
// ============================================================================
mod json {
    use std::fmt;

    #[derive(Debug, Clone)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        // Vec to preserve key order (first-seen). Object keys are unique
        // by RFC 8259 best practice; on duplicates we keep the LAST.
        Object(Vec<(String, Value)>),
    }

    impl Value {
        pub fn as_f64(&self) -> Option<f64> {
            match self {
                Value::Number(n) => Some(*n),
                Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                Value::String(s) => s.parse::<f64>().ok(),
                _ => None,
            }
        }

        /// Render this value as a JSON-style string. Used to serialise
        /// nested objects/arrays into Str-typed columns.
        pub fn serialize(&self) -> String {
            let mut out = String::new();
            ser(&mut out, self);
            out
        }
    }

    fn ser(out: &mut String, v: &Value) {
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e16 && n.is_finite() {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Value::String(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => {
                            out.push_str(&format!("\\u{:04x}", c as u32));
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    ser(out, item);
                }
                out.push(']');
            }
            Value::Object(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    ser(out, &Value::String(k.clone()));
                    out.push(':');
                    ser(out, v);
                }
                out.push('}');
            }
        }
    }

    /// Parser state.
    struct Parser<'a> {
        input: &'a [u8],
        pos: usize,
    }

    #[derive(Debug)]
    pub struct ParseError {
        message: String,
        position: usize,
    }

    impl fmt::Display for ParseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} at byte {}", self.message, self.position)
        }
    }

    pub fn parse(input: &str) -> Result<Value, ParseError> {
        let mut p = Parser {
            input: input.as_bytes(),
            pos: 0,
        };
        p.skip_ws();
        let v = p.parse_value()?;
        p.skip_ws();
        if p.pos != p.input.len() {
            return Err(p.err("trailing data after JSON value"));
        }
        Ok(v)
    }

    impl<'a> Parser<'a> {
        fn err(&self, msg: &str) -> ParseError {
            ParseError {
                message: msg.to_string(),
                position: self.pos,
            }
        }

        fn peek(&self) -> Option<u8> {
            self.input.get(self.pos).copied()
        }

        fn bump(&mut self) -> Option<u8> {
            let b = self.peek();
            if b.is_some() {
                self.pos += 1;
            }
            b
        }

        fn skip_ws(&mut self) {
            while let Some(b) = self.peek() {
                if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }

        fn parse_value(&mut self) -> Result<Value, ParseError> {
            self.skip_ws();
            match self.peek() {
                Some(b'{') => self.parse_object(),
                Some(b'[') => self.parse_array(),
                Some(b'"') => self.parse_string().map(Value::String),
                Some(b't') | Some(b'f') => self.parse_bool(),
                Some(b'n') => self.parse_null(),
                Some(b) if b == b'-' || b.is_ascii_digit() => self.parse_number(),
                Some(_) => Err(self.err("unexpected character")),
                None => Err(self.err("unexpected end of input")),
            }
        }

        // Consume a single byte iff it matches `byte`; otherwise return
        // a descriptive parse error. Named to avoid grep collisions with
        // `Option::expect` / `Result::expect` (CLAUDE.md §6.2).
        fn consume_byte(&mut self, byte: u8, label: &str) -> Result<(), ParseError> {
            if self.peek() == Some(byte) {
                self.pos += 1;
                Ok(())
            } else {
                Err(self.err(&format!("expected {}", label)))
            }
        }

        fn parse_object(&mut self) -> Result<Value, ParseError> {
            self.consume_byte(b'{', "'{'")?;
            self.skip_ws();
            let mut pairs: Vec<(String, Value)> = Vec::new();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(Value::Object(pairs));
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                self.consume_byte(b':', "':'")?;
                self.skip_ws();
                let value = self.parse_value()?;
                // last-write-wins on duplicate keys
                if let Some(existing) = pairs.iter_mut().find(|(k, _)| k == &key) {
                    existing.1 = value;
                } else {
                    pairs.push((key, value));
                }
                self.skip_ws();
                match self.bump() {
                    Some(b',') => continue,
                    Some(b'}') => break,
                    _ => return Err(self.err("expected ',' or '}' in object")),
                }
            }
            Ok(Value::Object(pairs))
        }

        fn parse_array(&mut self) -> Result<Value, ParseError> {
            self.consume_byte(b'[', "'['")?;
            self.skip_ws();
            let mut items: Vec<Value> = Vec::new();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(Value::Array(items));
            }
            loop {
                self.skip_ws();
                items.push(self.parse_value()?);
                self.skip_ws();
                match self.bump() {
                    Some(b',') => continue,
                    Some(b']') => break,
                    _ => return Err(self.err("expected ',' or ']' in array")),
                }
            }
            Ok(Value::Array(items))
        }

        fn parse_string(&mut self) -> Result<String, ParseError> {
            self.consume_byte(b'"', "'\"'")?;
            let mut out = String::new();
            loop {
                match self.bump() {
                    None => return Err(self.err("unterminated string")),
                    Some(b'"') => return Ok(out),
                    Some(b'\\') => match self.bump() {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(b'r') => out.push('\r'),
                        Some(b'b') => out.push('\u{0008}'),
                        Some(b'f') => out.push('\u{000C}'),
                        Some(b'u') => {
                            let mut cp: u32 = 0;
                            for _ in 0..4 {
                                let b = self.bump().ok_or_else(|| self.err("bad \\u escape"))?;
                                cp = cp * 16
                                    + match b {
                                        b'0'..=b'9' => (b - b'0') as u32,
                                        b'a'..=b'f' => (b - b'a' + 10) as u32,
                                        b'A'..=b'F' => (b - b'A' + 10) as u32,
                                        _ => return Err(self.err("bad \\u hex digit")),
                                    };
                            }
                            if let Some(c) = char::from_u32(cp) {
                                out.push(c);
                            } else {
                                out.push('\u{FFFD}');
                            }
                        }
                        _ => return Err(self.err("bad escape")),
                    },
                    Some(b) => {
                        // collect bytes until we have a complete UTF-8
                        // codepoint; for simplicity push as-is (we know
                        // the input is &str = valid UTF-8).
                        out.push(b as char);
                    }
                }
            }
        }

        fn parse_bool(&mut self) -> Result<Value, ParseError> {
            if self.input[self.pos..].starts_with(b"true") {
                self.pos += 4;
                Ok(Value::Bool(true))
            } else if self.input[self.pos..].starts_with(b"false") {
                self.pos += 5;
                Ok(Value::Bool(false))
            } else {
                Err(self.err("expected 'true' or 'false'"))
            }
        }

        fn parse_null(&mut self) -> Result<Value, ParseError> {
            if self.input[self.pos..].starts_with(b"null") {
                self.pos += 4;
                Ok(Value::Null)
            } else {
                Err(self.err("expected 'null'"))
            }
        }

        fn parse_number(&mut self) -> Result<Value, ParseError> {
            let start = self.pos;
            if self.peek() == Some(b'-') {
                self.pos += 1;
            }
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.peek() == Some(b'.') {
                self.pos += 1;
                while let Some(b) = self.peek() {
                    if b.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            if matches!(self.peek(), Some(b'e') | Some(b'E')) {
                self.pos += 1;
                if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                    self.pos += 1;
                }
                while let Some(b) = self.peek() {
                    if b.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            let slice = &self.input[start..self.pos];
            let s =
                std::str::from_utf8(slice).map_err(|_| self.err("number literal is not UTF-8"))?;
            let n = s.parse::<f64>().map_err(|_| self.err("invalid number"))?;
            Ok(Value::Number(n))
        }
    }
}
