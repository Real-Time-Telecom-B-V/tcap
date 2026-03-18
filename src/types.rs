use std::fmt;

use rasn::prelude::*;

/// TCAP Transaction ID (1-4 bytes).
pub type TransactionId = rasn::types::OctetString;

/// Operation Code — local or global.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum OperationCode {
    Local(i64),
    Global(rasn::types::ObjectIdentifier),
}

impl fmt::Display for OperationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(v) => write!(f, "local({v})"),
            Self::Global(oid) => write!(f, "global({oid})"),
        }
    }
}

/// Error Code — local or global.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum ErrorCode {
    Local(i64),
    Global(rasn::types::ObjectIdentifier),
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(v) => write!(f, "local({v})"),
            Self::Global(oid) => write!(f, "global({oid})"),
        }
    }
}

/// Invoke ID.
pub type InvokeId = i64;
