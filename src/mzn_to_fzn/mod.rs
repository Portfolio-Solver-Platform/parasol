mod compilation;
pub mod compilation_manager;

pub use compilation::*;

use std::path::Path;

use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct Conversion {
    fzn_file: NamedTempFile,
    ozn_file: NamedTempFile,
}

impl Conversion {
    pub fn fzn(&self) -> &Path {
        self.fzn_file.path()
    }

    pub fn ozn(&self) -> &Path {
        self.ozn_file.path()
    }
}
