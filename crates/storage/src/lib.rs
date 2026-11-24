//! Storage adapters for the Segno domain ports.
//!
//! Today: SQLite for the local library. A PostgreSQL implementation of the same
//! ports will back the cloud deployment.

mod sqlite;

pub use sqlite::SqliteStore;
