// SPDX-License-Identifier: Apache-2.0
use std::path::PathBuf;
use thiserror::Error;

/// Everything that can go wrong with a capture project.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("file problem: {0}")]
    Io(#[from] std::io::Error),
    #[error("the file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{} is not a capture project (no capture.json)", .0.display())]
    NotAProject(PathBuf),
    #[error("{} already holds a capture project", .0.display())]
    AlreadyExists(PathBuf),
    #[error("this project uses format version {0}, which this program cannot read")]
    UnsupportedVersion(String),
    #[error("{0}")]
    BadFormat(String),
}
