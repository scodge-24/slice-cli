use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not inside a git repository; set --repo")]
    NotInRepo,

    #[error("unknown slice: {0}")]
    UnknownSlice(String),

    #[error("{path}: missing YAML frontmatter")]
    MissingFrontmatter { path: String },

    #[error("{path}: missing `slice_id`")]
    MissingSliceId { path: String },

    #[error("failed to parse {path}: {source}")]
    Yaml {
        path: String,
        #[source]
        source: yaml_serde::Error,
    },

    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write output: {0}")]
    Output(#[from] serde_json::Error),

    #[error("{0}")]
    InvalidInput(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
