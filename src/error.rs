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

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

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
            OuraError::IoError(_) => -32603,
            OuraError::JsonError(_) => -32603,
            OuraError::Internal(_) => -32603,
        }
    }
}

pub type Result<T> = std::result::Result<T, OuraError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(OuraError::LoopAlreadyRunning.code(), -32001);
        assert_eq!(OuraError::NoActiveLoop.code(), -32002);
        assert_eq!(OuraError::LoopNotRunning("test".into()).code(), -32003);
        assert_eq!(OuraError::BackgroundLoopRunning.code(), -32004);
        assert_eq!(OuraError::Internal("i".into()).code(), -32603);
    }

    #[test]
    fn test_error_messages() {
        assert_eq!(OuraError::LoopAlreadyRunning.to_string(), "Loop already running");
        assert!(OuraError::LoopNotRunning("stopped".into()).to_string().contains("stopped"));
        assert!(OuraError::Internal("critical".into()).to_string().contains("critical"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let oura_err: OuraError = io_err.into();
        assert_eq!(oura_err.code(), -32603);
    }

    #[test]
    fn test_result_type() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());
        let err: Result<i32> = Err(OuraError::NoActiveLoop);
        assert!(err.is_err());
    }
}
