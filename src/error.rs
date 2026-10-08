use crate::decode::DecodeProblem;

/// Errors that can occur during TCAP message processing.
#[derive(Debug, thiserror::Error)]
pub enum TcapError {
    /// A received message was not fully understood. The problem says which
    /// sub-layer detected it, the P-Abort cause or the Reject problem code,
    /// and the transaction and invoke IDs that could be recovered, so the
    /// response Q.774 requires can be built: see [`DecodeProblem::abort`] and
    /// [`DecodeProblem::reject`].
    #[error("malformed message: {0}")]
    Malformed(Box<DecodeProblem>),

    #[error("BER decode error: {0}")]
    DecodeError(String),

    #[error("BER encode error: {0}")]
    EncodeError(String),

    /// A message handed to [`crate::encode`] that Q.773 does not allow.
    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("missing field: {0}")]
    MissingField(String),
}

impl TcapError {
    /// The decode problem, for [`TcapError::Malformed`].
    pub fn problem(&self) -> Option<&DecodeProblem> {
        match self {
            Self::Malformed(problem) => Some(problem),
            _ => None,
        }
    }
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
