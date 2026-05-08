//! PPTX table extractor — hand-rolled XML parsing for PowerPoint files.
//!
//! Extracts tables from PPTX slides and converts them to `ParsedCsv` format
//! so they can flow through the same detection/ingest pipeline as CSV uploads.
//! PPTX files are ZIP archives containing XML slides at `ppt/slides/slideN.xml`.

use crate::upload::ParsedCsv;
use std::io::Cursor;

/// Extract tables from a PPTX file, returning one `ParsedCsv` per table found.
///
/// Opens the PPTX as a ZIP archive, iterates slides in order, finds `<a:tbl>`
/// elements, and extracts rows/cells. The slide title (nearest preceding text
/// shape) becomes the filename for registry matching.
pub fn extract_pptx(bytes: &[u8]) -> Result<Vec<ParsedCsv>, String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("invalid PPTX/zip file: {e}"))?;

    // Collect slide entry names and sort by slide number.
    let mut slide_names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;
        let name = file.name().to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            slide_names.push(name);
        }
    }

    // Sort by slide number (slide1.xml, slide2.xml, ..., slide10.xml, ...)
    slide_names.sort_by_key(|name| {
        let num_part = name
            .trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml");
        num_part.parse::<u32>().unwrap_or(0)
    });

    let mut csvs = Vec::new();

    for slide_name in &slide_names {
        let mut file = archive
            .by_name(slide_name)
            .map_err(|e| format!("reading {slide_name}: {e}"))?;

        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)
            .map_err(|e| format!("reading {slide_name}: {e}"))?;

        // Extract slide number from filename (e.g., "ppt/slides/slide3.xml" → 3).
        let slide_num = slide_name
            .trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0);

        // Extract tables from this slide's XML.
        let tables = extract_tables_from_slide(&content, slide_num);
        csvs.extend(tables);
    }

    Ok(csvs)
}

/// Check if a byte slice looks like a PPTX file (ZIP with ppt/slides/ entries).
pub fn is_pptx(bytes: &[u8]) -> bool {
    // Must start with PK (ZIP magic bytes)
    if bytes.len() < 4 || bytes[0] != b'P' || bytes[1] != b'K' {
        return false;
    }
    // Try to open as ZIP and check for ppt/slides/ entries
    let cursor = Cursor::new(bytes);
    if let Ok(mut archive) = zip::ZipArchive::new(cursor) {
        for i in 0..archive.len().min(50) {
            if let Ok(file) = archive.by_index(i) {
                if file.name().starts_with("ppt/slides/") {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract all tables from a single slide's XML content.
/// `slide_num` is included in the filename to disambiguate tables from
/// different slides that happen to share the same title.
fn extract_tables_from_slide(xml: &str, slide_num: u32) -> Vec<ParsedCsv> {
    let mut results = Vec::new();

    // Extract the slide title (text from shapes preceding tables).
    let title = extract_slide_title(xml);

    // Split on <a:tbl to find table blocks.
    let table_splits: Vec<&str> = xml.split("<a:tbl").collect();
    let table_count = table_splits.len() - 1; // first split is pre-table content

    // First split is everything before the first table — skip it.
    for (table_idx, table_chunk) in table_splits.iter().enumerate().skip(1) {
        // Find the end of this table block.
        let table_xml = if let Some(end_pos) = table_chunk.find("</a:tbl>") {
            &table_chunk[..end_pos]
        } else {
            // No closing tag found — malformed, skip.
            continue;
        };

        let raw_rows = extract_table_rows(table_xml);
        if raw_rows.is_empty() {
            continue;
        }

        // Post-process: clean up text (collapse whitespace, decode XML entities)
        // and remove empty columns (PPTX tables often have empty spacer columns).
        let rows = clean_table_rows(raw_rows);
        if rows.is_empty() {
            continue;
        }

        // First row is headers, rest are data rows.
        let headers = rows[0].clone();
        let data_rows: Vec<Vec<String>> = rows[1..].to_vec();

        // Skip tables with no data rows or no headers.
        if headers.is_empty() || data_rows.is_empty() {
            continue;
        }

        // Build filename from title. Include slide number to disambiguate
        // when multiple slides share the same title text.
        let table_name = if !title.is_empty() {
            if table_count > 1 {
                // Multiple tables on one slide — include table index.
                format!("{}-slide{}-t{}", title, slide_num, table_idx)
            } else {
                format!("{}-slide{}", title, slide_num)
            }
        } else {
            format!("slide{}-table{}", slide_num, table_idx)
        };

        let filename = sanitize_filename(&table_name);

        results.push(ParsedCsv {
            filename,
            headers,
            rows: data_rows,
        });
    }

    results
}

/// Extract the slide title — looks for text in shapes that precede tables.
/// Targets section header text like "Display Ads - Overall Performance" or
/// "Display - Product Performance".
fn extract_slide_title(xml: &str) -> String {
    // Strategy: find text content from <p:sp> shapes that appear before any <a:tbl>.
    // We look for the most descriptive title text (longest non-trivial text).
    let before_table = if let Some(pos) = xml.find("<a:tbl") {
        &xml[..pos]
    } else {
        return String::new();
    };

    // Extract all text runs from shapes in the pre-table area.
    let mut candidate_titles: Vec<String> = Vec::new();

    // Find all <p:sp> shape blocks.
    let shape_splits: Vec<&str> = before_table.split("<p:sp>").collect();
    for shape_chunk in shape_splits.iter().skip(1) {
        let shape_end = shape_chunk.find("</p:sp>").unwrap_or(shape_chunk.len());
        let shape_xml = &shape_chunk[..shape_end];

        // Collect all <a:t> text within this shape.
        let text = collect_text_runs(shape_xml);
        let trimmed = text.trim().to_string();

        // Filter out very short text (single chars, numbers only) and common noise.
        if trimmed.len() > 3
            && !trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '%')
            && !trimmed.eq_ignore_ascii_case("title")
        {
            candidate_titles.push(trimmed);
        }
    }

    // Also check <p:sp elements with other formats (self-closing start or attributes).
    let alt_splits: Vec<&str> = before_table.split("<p:sp ").collect();
    for shape_chunk in alt_splits.iter().skip(1) {
        let shape_end = shape_chunk.find("</p:sp>").unwrap_or(shape_chunk.len());
        let shape_xml = &shape_chunk[..shape_end];

        let text = collect_text_runs(shape_xml);
        let trimmed = text.trim().to_string();

        if trimmed.len() > 3
            && !trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '%')
            && !trimmed.eq_ignore_ascii_case("title")
        {
            candidate_titles.push(trimmed);
        }
    }

    // Prefer a title that contains " - " (section header pattern) or is the longest.
    if let Some(sectioned) = candidate_titles.iter().filter(|t| t.contains(" - ")).last() {
        return sectioned.clone();
    }

    // Otherwise pick the last substantial candidate (closest to the table).
    candidate_titles
        .iter()
        .filter(|t| t.len() > 5)
        .last()
        .cloned()
        .unwrap_or_default()
}

/// Extract rows from a table XML block (content after `<a:tbl`).
fn extract_table_rows(table_xml: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();

    // Split on <a:tr to find row blocks.
    let row_splits: Vec<&str> = table_xml.split("<a:tr").collect();
    for row_chunk in row_splits.iter().skip(1) {
        let row_end = row_chunk.find("</a:tr>").unwrap_or(row_chunk.len());
        let row_xml = &row_chunk[..row_end];

        let cells = extract_row_cells(row_xml);
        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    rows
}

/// Extract cells from a row XML block (content after `<a:tr`).
fn extract_row_cells(row_xml: &str) -> Vec<String> {
    let mut cells = Vec::new();

    // Split on <a:tc to find cell blocks.
    let cell_splits: Vec<&str> = row_xml.split("<a:tc").collect();
    for cell_chunk in cell_splits.iter().skip(1) {
        let cell_end = cell_chunk.find("</a:tc>").unwrap_or(cell_chunk.len());
        let cell_xml = &cell_chunk[..cell_end];

        // Collect all <a:t> text within this cell.
        let text = collect_text_runs(cell_xml);
        cells.push(text.trim().to_string());
    }

    cells
}

/// Collect all `<a:t>...</a:t>` text content from an XML fragment.
/// Concatenates multiple text runs, joining paragraphs with a space.
fn collect_text_runs(xml: &str) -> String {
    let mut result = String::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<a:t>") {
        let text_start = search_from + start + 5; // len("<a:t>")
        if let Some(end) = xml[text_start..].find("</a:t>") {
            let text = &xml[text_start..text_start + end];
            if !result.is_empty() && !text.is_empty() {
                result.push(' ');
            }
            result.push_str(text);
            search_from = text_start + end + 6; // len("</a:t>")
        } else {
            break;
        }
    }

    // Also handle <a:t/> (self-closing — empty text) and
    // <a:t xml:space="preserve">...</a:t> variants.
    let mut search_from2 = 0;
    let preserve_tag = "<a:t xml:space=\"preserve\">";
    while let Some(start) = xml[search_from2..].find(preserve_tag) {
        let text_start = search_from2 + start + preserve_tag.len();
        if let Some(end) = xml[text_start..].find("</a:t>") {
            let text = &xml[text_start..text_start + end];
            if !result.is_empty() && !text.is_empty() {
                result.push(' ');
            }
            result.push_str(text);
            search_from2 = text_start + end + 6;
        } else {
            break;
        }
    }

    result
}

/// Clean up extracted table rows: collapse whitespace, decode XML entities,
/// and remove columns that are entirely empty (spacer columns in PPTX tables).
fn clean_table_rows(rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    if rows.is_empty() {
        return rows;
    }

    let col_count = rows[0].len();

    // Identify which columns are entirely empty across all rows.
    let mut empty_cols = vec![true; col_count];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count && !cell.trim().is_empty() {
                empty_cols[i] = false;
            }
        }
    }

    // Clean each row: decode entities, collapse whitespace, drop empty columns.
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .filter(|(i, _)| *i < col_count && !empty_cols[*i])
                .map(|(_, cell)| clean_cell_text(&cell))
                .collect()
        })
        .collect()
}

/// Clean a single cell's text: decode XML entities and collapse multiple spaces.
fn clean_cell_text(text: &str) -> String {
    let decoded = decode_xml_entities(text);
    // Collapse multiple whitespace characters into a single space.
    let mut result = String::with_capacity(decoded.len());
    let mut prev_space = false;
    for ch in decoded.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

/// Decode common XML entities.
fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Sanitize a title string into a filename suitable for registry matching.
/// E.g., "Display - Product Performance" → "display-product-performance.csv"
fn sanitize_filename(title: &str) -> String {
    let lower = title.to_lowercase();
    let sanitized: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse multiple dashes and trim.
    let mut result = String::new();
    let mut prev_dash = false;
    for ch in sanitized.chars() {
        if ch == '-' {
            if !prev_dash && !result.is_empty() {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(ch);
            prev_dash = false;
        }
    }

    // Trim trailing dash.
    let trimmed = result.trim_end_matches('-');
    format!("{}.csv", trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            sanitize_filename("Display - Product Performance"),
            "display-product-performance.csv"
        );
        assert_eq!(
            sanitize_filename("Display Ads - Overall Performance"),
            "display-ads-overall-performance.csv"
        );
        assert_eq!(sanitize_filename("SEM Results"), "sem-results.csv");
    }

    #[test]
    fn test_collect_text_runs() {
        let xml = r#"<a:t>Hello</a:t><a:t> World</a:t>"#;
        assert_eq!(collect_text_runs(xml), "Hello  World");
    }

    #[test]
    fn test_collect_text_runs_with_preserve() {
        let xml = r#"<a:t xml:space="preserve">Hello World</a:t>"#;
        assert_eq!(collect_text_runs(xml), "Hello World");
    }

    #[test]
    fn test_extract_row_cells() {
        let row = r#" h="370840"><a:tc><a:txBody><a:p><a:t>Metric</a:t></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:t>Value</a:t></a:p></a:txBody></a:tc>"#;
        let cells = extract_row_cells(row);
        assert_eq!(cells, vec!["Metric", "Value"]);
    }

    #[test]
    fn test_is_not_pptx() {
        assert!(!is_pptx(b"not a zip file"));
        assert!(!is_pptx(b"PK\x03\x04")); // ZIP but no ppt/slides/
    }

    /// Integration test against the real Lumina PPTX — only runs if the file exists.
    #[test]
    fn test_real_pptx_extraction() {
        let path = "/Users/edwinlovettiii/Downloads/1778249994166_lumina_charts.pptx";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("  [skip] {path} not found");
            return;
        };

        assert!(is_pptx(&bytes), "should detect as PPTX");

        let csvs = extract_pptx(&bytes).expect("extraction should succeed");
        assert!(!csvs.is_empty(), "should extract at least one table");

        eprintln!(
            "\n  Extracted {} tables from lumina_charts.pptx:",
            csvs.len()
        );
        for csv in &csvs {
            eprintln!(
                "    {} — {} cols × {} rows",
                csv.filename,
                csv.headers.len(),
                csv.rows.len()
            );
            eprintln!("      headers: {:?}", csv.headers);
            if let Some(row) = csv.rows.first() {
                eprintln!("      row[0]:  {:?}", row);
            }
        }
    }
}
