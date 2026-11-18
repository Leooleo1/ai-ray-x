//! raiox-core reads the local logs of AI coding agents into a private index
//! and answers where the tokens went. It keeps numbers and names only: no
//! conversation text, command text or file contents leave the parser.

pub mod advice;
pub mod efficiency;
pub mod model;
pub mod report;
pub mod scan;
pub mod secrets;
pub mod sources;
pub mod store;
mod util;

pub use scan::{Roots, ScanStats, refresh};
pub use store::Store;

/// Default index location, inside the platform cache folder.
pub fn default_index_path() -> Option<std::path::PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("raiox").join("index.sqlite"))
}
