use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(String),
    #[error("No compatible ADB device found. Ensure your device is in recovery/MiAssistant mode and connected via USB.")]
    DeviceNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Other: {0}")]
    Other(String),
}
