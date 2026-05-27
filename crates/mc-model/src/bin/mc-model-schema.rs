//! Phase 3K (ADR-0030 Decision 2): emit the JSON Schema for `ParsedModel`.
//!
//! Authors enable editor autocomplete by adding this directive at the top
//! of their YAML:
//!
//! ```yaml
//! # yaml-language-server: $schema=../../docs/specs/mosaic-model-schema.json
//! ```
//!
//! The committed `docs/specs/mosaic-model-schema.json` MUST match this
//! binary's output byte-for-byte. Regenerate after every schema change:
//!
//! ```sh
//! cargo run --bin mc-model-schema --quiet > docs/specs/mosaic-model-schema.json
//! ```
//!
//! CI drift check (per ADR-0030):
//!
//! ```sh
//! diff <(cargo run --bin mc-model-schema --quiet) docs/specs/mosaic-model-schema.json
//! ```

use mc_model::schema::ParsedModel;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(ParsedModel);
    match serde_json::to_string_pretty(&schema) {
        Ok(json) => {
            println!("{json}");
        }
        Err(e) => {
            eprintln!("schema serialization failed: {e}");
            std::process::exit(1);
        }
    }
}
