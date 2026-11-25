//! Storage adapters for the Segno domain ports.
//!
//! Today: SQLite for the library database and a folder for its media. PostgreSQL and object
//! storage implementations of the same ports will back the cloud deployment.

mod files;
mod sqlite;

pub use files::LibraryFiles;
pub use sqlite::SqliteStore;
