#![forbid(unsafe_code)]

pub mod check;
pub mod cli;
pub mod color;
pub mod commands;
pub mod config;
pub mod context;
pub mod error;
pub mod git_backend;
pub mod index;
pub mod manifest;
pub mod models;
pub mod paths;
#[cfg(feature = "semantic")]
pub mod semantic;
pub mod slices;
pub mod symbols;
#[cfg(feature = "ast")]
pub mod symbols_ast;

pub use error::{Error, Result};
