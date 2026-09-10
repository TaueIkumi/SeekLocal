mod discovery;
mod error;
mod extractor;
mod index;
mod model;

pub use error::{Error, Result};
pub use index::{SearchIndex, SemanticModel};
pub use model::{IndexReport, IndexStats, SearchResult, SemanticStatus};
