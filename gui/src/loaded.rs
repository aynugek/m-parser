//! The parsed statement currently shown in the window.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::SharedString;
use parser::Statement;

/// The file name of `path`, falling back to the full path when it has none.
pub fn file_name_for(path: &Path) -> SharedString {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
        .into()
}

/// A parsed statement together with the PDF it was read from.
#[derive(Debug, Clone)]
pub struct LoadedStatement {
    statement: Arc<Statement>,
    path: PathBuf,
}

impl LoadedStatement {
    /// Wraps a parsed statement and its source path.
    pub fn new(statement: Statement, path: PathBuf) -> Self {
        Self {
            statement: Arc::new(statement),
            path,
        }
    }

    /// The parsed statement.
    pub fn statement(&self) -> &Statement {
        &self.statement
    }

    /// The parsed statement, shared with a table or an export task.
    pub fn shared(&self) -> Arc<Statement> {
        self.statement.clone()
    }

    /// The PDF this statement was parsed from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The PDF's file name, for window titles and status text.
    pub fn file_name(&self) -> SharedString {
        file_name_for(&self.path)
    }
}
