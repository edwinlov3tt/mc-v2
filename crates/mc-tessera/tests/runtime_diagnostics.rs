//! Runtime diagnostics: MC5013 (credentials), MC5014 (file not found),
//! MC5015 (connection failure).

use std::fs;
use std::path::Path;

use mc_drivers::DriverError;
use mc_tessera::{Tessera, TesseraError};

#[test]
fn mc5014_fires_for_missing_csv() {
    let dir = make_tempdir("mc5014");
    copy_acme_assets(&dir);
    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: missing_csv
model: ./acme.yaml
source:
  driver: csv
  path: ./does-not-exist.csv
columns:
  - { source: Channel, dimension: Channel }
  - { source: Spend, measure: Spend }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
"#,
    )
    .unwrap();

    let err = Tessera::prepare(&dir.join("recipe.yaml")).unwrap_err();
    match err {
        TesseraError::Driver(DriverError::SourceFileNotFound { .. }) => {}
        other => panic!("expected SourceFileNotFound, got {other:?}"),
    }
    // diagnostic mapping
    let diag = TesseraError::driver_diagnostic(
        "/source/path",
        &DriverError::SourceFileNotFound {
            path: "/does/not/exist".into(),
            message: "noent".into(),
        },
    );
    assert_eq!(diag.code, "MC5014");
}

#[test]
fn mc5015_fires_for_connection_failure_diagnostic() {
    let diag = TesseraError::driver_diagnostic(
        "/source",
        &DriverError::ConnectionFailed {
            target: "localhost:5432".into(),
            message: "unreachable".into(),
        },
    );
    assert_eq!(diag.code, "MC5015");
}

#[test]
fn mc5013_fires_for_missing_env_credential() {
    let dir = make_tempdir("mc5013");
    copy_acme_assets(&dir);
    let key = "MC_TESSERA_NEVER_SET_99827";
    std::env::remove_var(key);
    fs::write(
        dir.join("recipe.yaml"),
        format!(
            r#"version: 1
name: missing_env
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - {{ source: Channel, dimension: Channel }}
  - {{ source: Spend, measure: Spend }}
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
credentials:
  TOKEN: "${{env.{key}}}"
"#,
        ),
    )
    .unwrap();
    fs::write(dir.join("inputs.csv"), "Channel,Spend\nPaid_Search,1\n").unwrap();

    let err = Tessera::prepare(&dir.join("recipe.yaml")).unwrap_err();
    let diag = match err {
        TesseraError::Secret { .. } => err.secret_diagnostic().expect("secret diag"),
        other => panic!("expected Secret error, got {other:?}"),
    };
    assert_eq!(diag.code, "MC5013");
}

fn copy_acme_assets(dir: &Path) {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("mc-model")
        .join("examples");
    fs::copy(examples_dir.join("acme.yaml"), dir.join("acme.yaml")).unwrap();
    fs::copy(
        examples_dir.join("acme.inputs.csv"),
        dir.join("acme.inputs.csv"),
    )
    .unwrap();
}

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("mc-tessera-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}
