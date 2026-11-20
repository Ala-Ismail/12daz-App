use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, PartialEq, Eq)]
pub enum ConversionError {
    Cancelled,
    Failed(String),
}

impl ConversionError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Cancelled."),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ConversionError {}

pub type Result<T> = std::result::Result<T, ConversionError>;

pub fn checkpoint(cancel: Option<&AtomicBool>) -> Result<()> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(ConversionError::Cancelled);
    }
    Ok(())
}
