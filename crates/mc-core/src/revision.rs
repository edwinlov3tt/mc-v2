//! Re-export `Revision` from `id` for forward-compat with snapshot/version
//! logic that may grow into its own module.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.2.

pub use crate::id::Revision;
