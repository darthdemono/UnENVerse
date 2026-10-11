//! Compatibility facade for callers that still import `unv_app`.
//!
//! The application engine lives in `unv_app`; this crate retains the historical
//! library name while the `unv` binary migrates to the application crate.

pub use unv_app::*;
