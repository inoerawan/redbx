//! Mobile FFI bindings for redbx.
//!
//! Exposes a UniFFI-based API for use from Android (Kotlin/Java) and iOS (Swift/ObjC).
//! Use [`RedbxDatabase::create`] or [`RedbxDatabase::open`] as entry points.
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
