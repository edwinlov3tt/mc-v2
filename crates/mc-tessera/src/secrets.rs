//! `SecretResolver` — the Grout forward-compat interface.
//!
//! Phase 5A ships only the trait + `EnvVarSecretResolver`. The future
//! `mc-grout` crate (Phase 5E) will provide additional resolvers
//! (vault-backed, rotation-aware, audit-logged) without changing the
//! call sites in `mc-tessera`.
//!
//! ## Reference syntax
//!
//! Phase 5A recognizes `${env.VAR_NAME}` references inside
//! `recipe.credentials` values. The resolver receives the contents
//! between the braces (e.g., `"env.PG_DSN"`) and returns the resolved
//! value. Anything not matching the `env.` scheme returns
//! [`SecretError::UnsupportedScheme`] — Phase 5E will add `secret.ref`,
//! `vault.path`, etc.

use std::env;

use thiserror::Error;

/// Failure modes for [`SecretResolver::resolve`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum SecretError {
    /// The environment variable named in a `${env.X}` reference is not
    /// set in the current process. Maps to MC5013 in
    /// [`crate::TesseraError::from_secret`].
    #[error("environment variable {variable:?} is not set")]
    EnvNotSet {
        /// The variable name.
        variable: String,
    },

    /// The reference scheme is not supported in the current Phase. Maps
    /// to MC5013 with an explanatory message.
    #[error("unsupported secret scheme: {scheme:?}")]
    UnsupportedScheme {
        /// The scheme as it appeared in the reference.
        scheme: String,
    },
}

/// The contract every secret resolver implements.
///
/// Resolvers are queried with the **inner** content of a `${...}`
/// reference, e.g., `"env.PG_DSN"`, `"secret.production/postgres"`. The
/// scheme is the substring before the first `.`. Resolvers return either
/// the resolved value or a [`SecretError`].
pub trait SecretResolver {
    /// Resolve `reference` to its concrete value. `reference` is the
    /// substring INSIDE the braces (no `${` / `}`).
    fn resolve(&self, reference: &str) -> Result<String, SecretError>;
}

/// Phase 5A's only resolver: `${env.NAME}` is read from the process
/// environment via [`std::env::var`].
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvVarSecretResolver;

impl SecretResolver for EnvVarSecretResolver {
    fn resolve(&self, reference: &str) -> Result<String, SecretError> {
        // Reference shapes accepted in Phase 5A:
        //   "env.NAME" → look up NAME in the environment.
        // Anything else (no scheme, unknown scheme) → UnsupportedScheme.
        if let Some(name) = reference.strip_prefix("env.") {
            match env::var(name) {
                Ok(v) => Ok(v),
                Err(_) => Err(SecretError::EnvNotSet {
                    variable: name.to_string(),
                }),
            }
        } else {
            Err(SecretError::UnsupportedScheme {
                scheme: reference
                    .split_once('.')
                    .map(|(s, _)| s.to_string())
                    .unwrap_or_else(|| reference.to_string()),
            })
        }
    }
}

/// Walk a string, replacing every `${ref}` occurrence with the resolver's
/// output. Non-reference text is returned unchanged. Returns the first
/// resolver error encountered.
///
/// Recognized syntax (Phase 5A): `${...}` with no nesting. A literal `$`
/// is preserved as-is when not followed by `{`. The closing `}` is
/// required; a `${` with no `}` aborts with [`SecretError::UnsupportedScheme`]
/// carrying the malformed fragment as a hint.
pub fn interpolate<R: SecretResolver>(raw: &str, resolver: &R) -> Result<String, SecretError> {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Find the matching '}'. No nesting in Phase 5A.
            let start = i + 2;
            let end_rel = match raw[start..].find('}') {
                Some(j) => j,
                None => {
                    return Err(SecretError::UnsupportedScheme {
                        scheme: format!("malformed reference: {}", &raw[i..]),
                    });
                }
            };
            let reference = &raw[start..start + end_rel];
            out.push_str(&resolver.resolve(reference)?);
            i = start + end_rel + 1;
        } else {
            // Push next char; advance by its UTF-8 length.
            let ch_len = utf8_char_len(bytes[i]);
            out.push_str(&raw[i..i + ch_len]);
            i += ch_len;
        }
    }
    Ok(out)
}

/// Length in bytes of the UTF-8 code point starting at `lead`.
fn utf8_char_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        // Continuation byte should not appear at lead position; treat
        // as 1 to make forward progress.
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_resolver_returns_value() {
        // Pick an env var that's nearly always present on test runners.
        // PATH on Unix, USERPROFILE on Windows; fall back to set-and-test
        // if neither exists.
        let key = "MC_TESSERA_TEST_VAR";
        std::env::set_var(key, "hello");
        let r = EnvVarSecretResolver;
        assert_eq!(r.resolve(&format!("env.{key}")).unwrap(), "hello");
        std::env::remove_var(key);
    }

    #[test]
    fn env_var_resolver_returns_env_not_set() {
        let key = "MC_TESSERA_DEFINITELY_NOT_SET_98675";
        std::env::remove_var(key);
        let r = EnvVarSecretResolver;
        assert_eq!(
            r.resolve(&format!("env.{key}")),
            Err(SecretError::EnvNotSet {
                variable: key.to_string()
            })
        );
    }

    #[test]
    fn unsupported_scheme_for_secret_ref() {
        let r = EnvVarSecretResolver;
        assert!(matches!(
            r.resolve("secret.foo"),
            Err(SecretError::UnsupportedScheme { .. })
        ));
    }

    #[test]
    fn interpolate_replaces_single_reference() {
        let key = "MC_TESSERA_INTERP_X";
        std::env::set_var(key, "world");
        let out = interpolate(&format!("hello ${{env.{key}}}!"), &EnvVarSecretResolver).unwrap();
        assert_eq!(out, "hello world!");
        std::env::remove_var(key);
    }

    #[test]
    fn interpolate_passes_through_when_no_reference() {
        let out = interpolate("plain text", &EnvVarSecretResolver).unwrap();
        assert_eq!(out, "plain text");
    }

    #[test]
    fn interpolate_returns_error_when_var_missing() {
        let key = "MC_TESSERA_MISSING_INTERP";
        std::env::remove_var(key);
        let r = interpolate(&format!("${{env.{key}}}"), &EnvVarSecretResolver);
        assert!(matches!(r, Err(SecretError::EnvNotSet { .. })));
    }
}
