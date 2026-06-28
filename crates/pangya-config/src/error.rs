use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read ini file: {0}")]
    IniRead(#[from] ini::Error),

    #[error("missing required section [{0}]")]
    MissingSection(String),

    #[error("missing required key {1} in section [{0}]")]
    MissingKey(String, String),

    #[error("invalid value for key {1} in section [{0}]")]
    BadValue(String, String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
