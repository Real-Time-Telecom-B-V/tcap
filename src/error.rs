/// Errors that can occur during TCAP message processing.
#[derive(Debug, thiserror::Error)]
pub enum TcapError {
    #[error("BER decode error: {0}")]
    DecodeError(String),

    #[error("BER encode error: {0}")]
    EncodeError(String),

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("missing field: {0}")]
    MissingField(String),
}

impl From<rasn::error::DecodeError> for TcapError {
    fn from(e: rasn::error::DecodeError) -> Self {
        Self::DecodeError(format!("{e}"))
    }
}

impl From<rasn::error::EncodeError> for TcapError {
    fn from(e: rasn::error::EncodeError) -> Self {
        Self::EncodeError(format!("{e}"))
    }
}
