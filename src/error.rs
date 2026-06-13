use thiserror::Error;

#[derive(Error, Debug)]
pub enum OuraError {
    #[error("Loop already running")]
    LoopAlreadyRunning,

    #[error("No active loop")]
    NoActiveLoop,

    #[error("Loop not running (status: {0})")]
    LoopNotRunning(String),

    #[error("Background loop running, stop it first")]
    BackgroundLoopRunning,

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Command timed out after {0} seconds")]
    CommandTimeout(u64),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl OuraError {
    pub fn code(&self) -> i32 {
        match self {
            OuraError::LoopAlreadyRunning => -32001,
            OuraError::NoActiveLoop => -32002,
            OuraError::LoopNotRunning(_) => -32003,
            OuraError::BackgroundLoopRunning => -32004,
            OuraError::CommandFailed(_) => -32010,
            OuraError::CommandTimeout(_) => -32011,
            OuraError::ParseError(_) => -32700,
            OuraError::ConfigError(_) => -32602,
            OuraError::IoError(_) => -32603,
            OuraError::JsonError(_) => -32603,
            OuraError::HttpError(_) => -32603,
            OuraError::Internal(_) => -32603,
        }
    }
}

pub type Result<T> = std::result::Result<T, OuraError>;
