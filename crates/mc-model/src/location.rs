//! Transitional YAML source-location side-table.
//!
//! // TODO(saphyr): replace with single-pass LocatedValue parsing.
//! // This module builds a `LocationMap` by scanning the raw YAML source text
//! // for key patterns and recording byte offsets. It is approximate (key-based
//! // text search, not AST-level positions) but sufficient for Phase 7A.6's
//! // diagnostic rendering. New diagnostic surfaces should NOT extend this —
//! // they should parse with `saphyr` directly.
//!
//! Per [ADR-0024](../../../docs/decisions/0024-rich-diagnostic-rendering.md)
//! Decision 4: transitional v1 path using line-scanning.

use std::collections::HashMap;

use mc_diagnostics::SourceSpan;

/// Maps yaml_pointer strings to byte-offset `SourceSpan`s in the source file.
///
/// // TODO(saphyr): replace with single-pass LocatedValue parsing.
#[derive(Debug)]
pub struct LocationMap {
    spans: HashMap<String, SourceSpan>,
    source_text: String,
    file_path: String,
}

impl LocationMap {
    /// Build a location map by scanning the YAML source text.
    ///
    /// // TODO(saphyr): replace with single-pass LocatedValue parsing.
    ///
    /// Uses `serde_yaml::Value` to walk the tree and a line-scanning approach
    /// to find byte offsets for YAML keys and values. The `serde_yaml` 0.9
    /// `Value` API does not expose per-node positions, so we fall back to
    /// text search for each key.
    pub fn build(file_path: &str, yaml_content: &str) -> Self {
        let mut map = LocationMap {
            spans: HashMap::new(),
            source_text: yaml_content.to_string(),
            file_path: file_path.to_string(),
        };

        // Parse into serde_yaml::Value to walk the tree structure
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml_content) {
            map.walk_value(&value, String::new(), yaml_content);
        }

        map
    }

    /// Look up the span for a yaml_pointer.
    pub fn get(&self, yaml_pointer: &str) -> Option<&SourceSpan> {
        self.spans.get(yaml_pointer)
    }

    /// Look up the body span for a rule by name.
    ///
    /// Searches for `/rules/{i}/body` entries where the corresponding
    /// `/rules/{i}/name` matches `rule_name`. Used for formula offset
    /// composition.
    ///
    /// // TODO(saphyr): replace with single-pass LocatedValue parsing.
    pub fn rule_body_span(&self, rule_name: &str) -> Option<&SourceSpan> {
        // Walk /rules/0, /rules/1, etc. looking for matching name
        for i in 0..100 {
            let name_ptr = format!("/rules/{}/name", i);
            let body_ptr = format!("/rules/{}/body", i);
            if let Some(name_span) = self.spans.get(&name_ptr) {
                let name_text = &self.source_text
                    [name_span.start_byte..name_span.end_byte.min(self.source_text.len())];
                if name_text == rule_name {
                    return self.spans.get(&body_ptr);
                }
            } else {
                break; // No more rules
            }
        }
        None
    }

    /// Get the source text.
    pub fn source_text(&self) -> &str {
        &self.source_text
    }

    /// Get the file path.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// Walk a serde_yaml::Value tree, building pointer → span mappings.
    ///
    /// // TODO(saphyr): precise positions via single-pass parser.
    fn walk_value(&mut self, value: &serde_yaml::Value, pointer: String, source: &str) {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (key, val) in map {
                    let key_str = match key {
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => continue,
                    };
                    let child_pointer = format!("{}/{}", pointer, key_str);

                    // Try to find the key's value in the source text
                    if let Some(span) = self.find_value_span(source, &key_str, &child_pointer, val)
                    {
                        self.spans.insert(child_pointer.clone(), span);
                    }

                    self.walk_value(val, child_pointer, source);
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for (i, val) in seq.iter().enumerate() {
                    let child_pointer = format!("{}/{}", pointer, i);

                    // For sequence items, try to find the item by context
                    // (e.g., a mapping with a "name" key)
                    if let Some(span) = self.find_sequence_item_span(source, val, i) {
                        self.spans.insert(child_pointer.clone(), span);
                    }

                    self.walk_value(val, child_pointer, source);
                }
            }
            _ => {
                // Scalar values — the pointer was already mapped when we
                // found the parent key's value.
            }
        }
    }

    /// Find the byte span of a YAML value by searching for its key in source.
    ///
    /// // TODO(saphyr): precise positions via single-pass parser.
    fn find_value_span(
        &self,
        source: &str,
        key: &str,
        _pointer: &str,
        value: &serde_yaml::Value,
    ) -> Option<SourceSpan> {
        // Search for "key:" pattern in the source. This is approximate but
        // works for well-structured YAML. We look for the value after the key.
        let key_pattern = format!("{}:", key);

        // Find all occurrences and use the right one based on context
        let mut search_from = 0;
        while let Some(key_pos) = source[search_from..].find(&key_pattern) {
            let abs_pos = search_from + key_pos;
            let after_key = abs_pos + key_pattern.len();

            // Skip whitespace after "key:"
            let value_start = source[after_key..]
                .find(|c: char| !c.is_whitespace() || c == '\n')
                .map(|off| after_key + off)
                .unwrap_or(after_key);

            // For string values, try to find the exact string
            if let serde_yaml::Value::String(s) = value {
                if let Some(str_start) = source[after_key..].find(s.as_str()) {
                    let abs_str_start = after_key + str_start;
                    return Some(SourceSpan::new(
                        &self.file_path,
                        abs_str_start,
                        abs_str_start + s.len(),
                    ));
                }
                // If we can't find the exact string (might be quoted/escaped),
                // try to find the quoted version
                let quoted_patterns = [format!("\"{}\"", s), format!("'{}'", s)];
                for quoted in &quoted_patterns {
                    if let Some(q_start) = source[after_key..].find(quoted.as_str()) {
                        let abs_q_start = after_key + q_start;
                        // Point at the content inside the quotes
                        return Some(SourceSpan::new(
                            &self.file_path,
                            abs_q_start + 1,
                            abs_q_start + 1 + s.len(),
                        ));
                    }
                }
            }

            // For non-string scalars, point at the value start
            if let Some(end) = find_value_end(source, value_start) {
                return Some(SourceSpan::new(&self.file_path, value_start, end));
            }

            search_from = abs_pos + 1;
        }

        None
    }

    /// Find the span of a sequence item (approximation).
    ///
    /// // TODO(saphyr): precise positions via single-pass parser.
    fn find_sequence_item_span(
        &self,
        _source: &str,
        _value: &serde_yaml::Value,
        _index: usize,
    ) -> Option<SourceSpan> {
        // Sequence item location is harder to find via text search.
        // For v1, we skip sequence-level spans. The key-level spans within
        // sequence items (e.g., /measures/0/body) are caught by walk_value.
        None
    }
}

/// Find the end of a scalar value starting at `start`.
fn find_value_end(source: &str, start: usize) -> Option<usize> {
    if start >= source.len() {
        return None;
    }

    let rest = &source[start..];

    // Quoted string
    if rest.starts_with('"') {
        let mut i = 1;
        while i < rest.len() {
            if rest.as_bytes()[i] == b'\\' {
                i += 2;
            } else if rest.as_bytes()[i] == b'"' {
                return Some(start + i + 1);
            } else {
                i += 1;
            }
        }
        return Some(start + rest.len());
    }

    // Single-quoted string
    if let Some(inner) = rest.strip_prefix('\'') {
        if let Some(end) = inner.find('\'') {
            return Some(start + 1 + end + 1);
        }
        return Some(start + rest.len());
    }

    // Unquoted value — ends at newline or end of file
    let end = rest
        .find('\n')
        .map(|pos| start + pos)
        .unwrap_or(start + rest.len());
    // Trim trailing whitespace/comments
    let trimmed = source[start..end].trim_end();
    Some(start + trimmed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_map_finds_string_value() {
        let yaml = "name: Revenue\nbody: \"Spend / Clicks\"";
        let map = LocationMap::build("model.yaml", yaml);

        let span = map.get("/body");
        assert!(span.is_some(), "should find /body span");
        let span = span.unwrap();
        // "Spend / Clicks" is the content inside the quotes
        let content = &yaml[span.start_byte..span.end_byte];
        assert_eq!(content, "Spend / Clicks");
    }

    #[test]
    fn location_map_finds_nested_key() {
        let yaml = "measures:\n  - name: Revenue\n    body: \"Spend * AOV\"";
        let map = LocationMap::build("model.yaml", yaml);

        // Inside a sequence item, the pointer is /measures/0/body
        let span = map.get("/measures/0/body");
        assert!(
            span.is_some(),
            "should find /measures/0/body span, keys: {:?}",
            map.spans.keys().collect::<Vec<_>>()
        );
        let span = span.unwrap();
        let content = &yaml[span.start_byte..span.end_byte];
        assert_eq!(content, "Spend * AOV");
    }

    #[test]
    fn location_map_handles_missing_pointer() {
        let yaml = "name: Revenue";
        let map = LocationMap::build("model.yaml", yaml);
        assert!(map.get("/nonexistent").is_none());
    }

    #[test]
    fn location_map_with_inner_offset_composes() {
        let yaml = "body: \"Custmers * AOV\"";
        let map = LocationMap::build("model.yaml", yaml);

        let body_span = map.get("/body").unwrap();
        // "Custmers" starts at offset 0 within the formula string
        let inner = body_span.with_inner_offset(0, 8);
        let content = &yaml[inner.start_byte..inner.end_byte];
        assert_eq!(content, "Custmers");
    }
}
