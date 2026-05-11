//! Daemon configuration — `daemon.toml` parsing + CLI flag merge.
//!
//! Per ADR-0029 Decision 11: configuration lives in `daemon.toml` at the
//! workspace root (or `.mosaic/daemon.toml`). CLI flags override TOML values.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Fully resolved daemon configuration (TOML defaults + CLI overrides applied).
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub host: IpAddr,
    pub port: u16,
    pub api_key: Option<String>,
    pub workspace_path: PathBuf,
    pub detach: bool,
    pub cache_budget_mb: usize,
    pub log_format: LogFormat,
    pub log_level: LogLevel,
    pub default_timeout_ms: u64,
    pub max_request_body_mb: usize,
    /// CORS origins. "auto" is resolved at server startup based on host binding.
    pub cors_origins: CorsConfig,
}

/// CORS configuration.
///
/// Per ADR-0029 Decision 11: "auto" mode allows localhost origins when bound
/// to localhost; empty (must configure) when non-localhost. Users can override
/// with explicit origin lists.
#[derive(Debug, Clone)]
pub enum CorsConfig {
    /// Auto-computed from host binding. Localhost → allow localhost origins.
    /// Non-localhost → empty (must be explicitly configured).
    Auto,
    /// Explicit list of allowed origins.
    Explicit(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Auto,
    Json,
    Pretty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl DaemonConfig {
    /// Build config from defaults, optionally overlaying a TOML file, then
    /// applying CLI flag overrides.
    pub fn resolve(cli: CliFlags) -> Result<Self, String> {
        let workspace_path = cli.workspace.clone().unwrap_or_else(|| PathBuf::from("."));

        // Try to load daemon.toml from workspace root or .mosaic/
        let toml = load_toml(&workspace_path);

        let host: IpAddr = cli
            .host
            .as_deref()
            .or(toml.as_ref().and_then(|t| t.host.as_deref()))
            .unwrap_or("127.0.0.1")
            .parse()
            .map_err(|e| format!("invalid --host address: {e}"))?;

        let port = cli
            .port
            .or(toml.as_ref().and_then(|t| t.port))
            .unwrap_or(8787);

        let api_key = cli.api_key.clone().or_else(|| {
            toml.as_ref()
                .and_then(|t| t.api_key.clone())
                .filter(|k| !k.is_empty())
        });

        let detach = cli.detach;

        let cache_budget_mb = toml.as_ref().and_then(|t| t.cache_budget_mb).unwrap_or(512);

        let log_format = toml
            .as_ref()
            .and_then(|t| t.log_format.as_deref())
            .map(parse_log_format)
            .unwrap_or(LogFormat::Auto);

        let log_level = toml
            .as_ref()
            .and_then(|t| t.log_level.as_deref())
            .map(parse_log_level)
            .unwrap_or(LogLevel::Info);

        let default_timeout_ms = toml
            .as_ref()
            .and_then(|t| t.default_timeout_ms)
            .unwrap_or(60_000);

        let max_request_body_mb = toml
            .as_ref()
            .and_then(|t| t.max_request_body_mb)
            .unwrap_or(10);

        let cors_origins = toml
            .as_ref()
            .and_then(|t| t.cors_origins.as_ref())
            .map(parse_cors)
            .unwrap_or(CorsConfig::Auto);

        // Per ADR-0029 Decision 7: refuse non-localhost bind without api_key.
        if !is_localhost(host) && api_key.is_none() {
            return Err(format!(
                "Refusing to bind to {host} without --api-key. \
                 Set an API key for network-exposed deployments."
            ));
        }

        Ok(DaemonConfig {
            host,
            port,
            api_key,
            workspace_path,
            detach,
            cache_budget_mb,
            log_format,
            log_level,
            default_timeout_ms,
            max_request_body_mb,
            cors_origins,
        })
    }
}

/// Raw CLI flags before merging with TOML.
#[derive(Debug, Default)]
pub struct CliFlags {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub api_key: Option<String>,
    pub workspace: Option<PathBuf>,
    pub detach: bool,
}

/// Parse `mc up` CLI arguments into raw flags.
pub fn parse_up_args(args: &[String]) -> Result<CliFlags, String> {
    let mut flags = CliFlags::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--host" => match iter.next() {
                Some(v) => flags.host = Some(v.clone()),
                None => return Err("--host requires an address".into()),
            },
            "--port" => match iter.next() {
                Some(v) => {
                    flags.port = Some(
                        v.parse()
                            .map_err(|_| format!("--port must be a number, got {v:?}"))?,
                    );
                }
                None => return Err("--port requires a number".into()),
            },
            "--api-key" => match iter.next() {
                Some(v) => flags.api_key = Some(v.clone()),
                None => return Err("--api-key requires a value".into()),
            },
            "--workspace" => match iter.next() {
                Some(v) => flags.workspace = Some(PathBuf::from(v)),
                None => return Err("--workspace requires a path".into()),
            },
            "--detach" => flags.detach = true,
            other => return Err(format!("unknown argument to `mc up`: {other:?}")),
        }
    }
    Ok(flags)
}

/// Check whether an address is localhost (127.0.0.1, ::1).
/// Per ADR-0029 Decision 7: anything else requires --api-key.
pub fn is_localhost(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

// ---------------------------------------------------------------------------
// TOML parsing (minimal hand-rolled — avoids adding `toml` crate dep)
// ---------------------------------------------------------------------------

/// Lightweight parsed TOML values. We parse only the fields we need.
#[derive(Debug, Default)]
struct TomlConfig {
    host: Option<String>,
    port: Option<u16>,
    api_key: Option<String>,
    cache_budget_mb: Option<usize>,
    log_format: Option<String>,
    log_level: Option<String>,
    default_timeout_ms: Option<u64>,
    max_request_body_mb: Option<usize>,
    cors_origins: Option<serde_json::Value>,
}

fn load_toml(workspace_path: &Path) -> Option<TomlConfig> {
    // Try workspace root first, then .mosaic/
    let candidates = [
        workspace_path.join("daemon.toml"),
        workspace_path.join(".mosaic").join("daemon.toml"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            return parse_toml_config(&content);
        }
    }
    None
}

/// Minimal TOML-like parser. We support only flat `key = value` and `[section]`
/// headers for the daemon config subset. Full TOML parsing would require a dep.
fn parse_toml_config(content: &str) -> Option<TomlConfig> {
    let mut config = TomlConfig::default();
    let mut section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            let full_key = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };
            match full_key.as_str() {
                "daemon.host" => config.host = Some(val.to_string()),
                "daemon.port" => config.port = val.parse().ok(),
                "daemon.api_key" => config.api_key = Some(val.to_string()),
                "cache.budget_mb" => config.cache_budget_mb = val.parse().ok(),
                "logging.format" => config.log_format = Some(val.to_string()),
                "logging.level" => config.log_level = Some(val.to_string()),
                "timeouts.default_ms" => config.default_timeout_ms = val.parse().ok(),
                "api.max_request_body_mb" => config.max_request_body_mb = val.parse().ok(),
                "api.cors_origins" => {
                    if val == "auto" {
                        config.cors_origins = Some(serde_json::Value::String("auto".to_string()));
                    }
                    // Explicit arrays would need full TOML parsing; for Phase 8.0
                    // "auto" is the only supported value.
                }
                _ => {} // Ignore unknown keys
            }
        }
    }
    Some(config)
}

fn parse_log_format(s: &str) -> LogFormat {
    match s {
        "json" => LogFormat::Json,
        "pretty" => LogFormat::Pretty,
        _ => LogFormat::Auto,
    }
}

fn parse_log_level(s: &str) -> LogLevel {
    match s {
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

fn parse_cors(v: &serde_json::Value) -> CorsConfig {
    match v {
        serde_json::Value::String(s) if s == "auto" => CorsConfig::Auto,
        serde_json::Value::Array(arr) => {
            let origins: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            CorsConfig::Explicit(origins)
        }
        _ => CorsConfig::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let flags = CliFlags::default();
        let config = DaemonConfig::resolve(flags).unwrap();
        assert_eq!(config.port, 8787);
        assert!(is_localhost(config.host));
        assert!(config.api_key.is_none());
        assert_eq!(config.cache_budget_mb, 512);
    }

    #[test]
    fn test_cli_overrides() {
        let flags = CliFlags {
            port: Some(9999),
            host: Some("127.0.0.1".into()),
            api_key: Some("test-key".into()),
            ..Default::default()
        };
        let config = DaemonConfig::resolve(flags).unwrap();
        assert_eq!(config.port, 9999);
        assert_eq!(config.api_key.as_deref(), Some("test-key"));
    }

    #[test]
    fn test_non_localhost_without_key_rejected() {
        let flags = CliFlags {
            host: Some("0.0.0.0".into()),
            ..Default::default()
        };
        let err = DaemonConfig::resolve(flags).unwrap_err();
        assert!(err.contains("Refusing to bind"));
    }

    #[test]
    fn test_non_localhost_with_key_accepted() {
        let flags = CliFlags {
            host: Some("0.0.0.0".into()),
            api_key: Some("my-key".into()),
            ..Default::default()
        };
        let config = DaemonConfig::resolve(flags).unwrap();
        assert!(!is_localhost(config.host));
    }

    #[test]
    fn test_ipv6_localhost() {
        assert!(is_localhost("::1".parse().unwrap()));
        assert!(!is_localhost("::".parse().unwrap()));
    }

    #[test]
    fn test_parse_toml_config() {
        let toml = r#"
[daemon]
port = 9000
host = "127.0.0.1"
api_key = "secret"

[cache]
budget_mb = 1024

[logging]
format = "json"
level = "debug"

[api]
cors_origins = "auto"
"#;
        let config = parse_toml_config(toml).unwrap();
        assert_eq!(config.port, Some(9000));
        assert_eq!(config.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.api_key.as_deref(), Some("secret"));
        assert_eq!(config.cache_budget_mb, Some(1024));
        assert_eq!(config.log_format.as_deref(), Some("json"));
        assert_eq!(config.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn test_parse_up_args() {
        let args: Vec<String> = vec!["--port", "9999", "--api-key", "abc", "--detach"]
            .into_iter()
            .map(String::from)
            .collect();
        let flags = parse_up_args(&args).unwrap();
        assert_eq!(flags.port, Some(9999));
        assert_eq!(flags.api_key.as_deref(), Some("abc"));
        assert!(flags.detach);
    }
}
