//! Mobile FFI bindings for redbx.
//!
//! Exposes a UniFFI-based API for use from Android (Kotlin/Java). iOS bindings
//! will be added once the build is running on macOS.
//!
//! Use [`RedbxDatabase::create`] or [`RedbxDatabase::open`] as entry points.
//!
//! # Threading and object lifetime
//!
//! Every method here is **blocking** and internally serialised by a mutex — call
//! them off the UI thread. Objects are reference-counted across the FFI boundary,
//! so a foreign caller must release them explicitly (`use {}` / `.destroy()` in
//! Kotlin). In particular, a [`RedbxWriteTransaction`] that is neither committed
//! nor aborted keeps redbx's single write slot held until it is released.
#![deny(clippy::all, clippy::pedantic, clippy::disallowed_methods)]
#![allow(
    clippy::default_trait_access,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

uniffi::setup_scaffolding!();

mod database;
mod multimap;
mod table;
mod transaction;
mod types;

pub use database::RedbxDatabase;
pub use multimap::{RedbxMultimapTable, RedbxReadOnlyMultimapTable};
pub use table::{RedbxReadOnlyTable, RedbxTable};
pub use transaction::{RedbxReadTransaction, RedbxWriteTransaction};
pub use types::{RedbxError, RedbxKeyValue, RedbxValue};
