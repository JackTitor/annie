use std::{io, path::PathBuf};

use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AnnieError {
    #[error("cannot read config file at {path}: {source:?}")]
    LoadConfigError {
        source: anyhow::Error,
        path: PathBuf,
    },
    #[error("cannot write config file to {path}: {source:?}")]
    SaveConfigError {
        source: anyhow::Error,
        path: PathBuf,
    },
    #[error("cannot show config file in explorer at {path}: {source:?}")]
    ShowConfigError { source: io::Error, path: PathBuf },
}

pub type AnnieResult<T> = Result<T, AnnieError>;
