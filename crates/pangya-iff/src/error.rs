use thiserror::Error;

#[derive(Debug, Error)]
pub enum IffError {
    #[error("IFF entry header too short: got {got} bytes, need {need}")]
    ShortHeader { got: usize, need: usize },

    #[error("unsupported IFF version: got {version:#x}, expected {expected:#x}")]
    UnsupportedVersion { version: u32, expected: u32 },

    #[error(
        "entry {entry:?} size mismatch: expected {expected} \
         (head + {count} records x {record_size}B), got {actual}"
    )]
    SizeMismatch {
        entry: String,
        expected: usize,
        actual: usize,
        count: u16,
        record_size: usize,
    },

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
