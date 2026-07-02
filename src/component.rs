use std::fmt;

use rasn::prelude::*;

use crate::types::{ErrorCode, InvokeId, OperationCode};

/// TCAP Component types per ITU-T Q.773 Section 3.2.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum Component {
    #[rasn(tag(context, 1))]
    Invoke(Invoke),
    #[rasn(tag(context, 2))]
    ReturnResultLast(ReturnResult),
    #[rasn(tag(context, 3))]
    ReturnError(ReturnError),
    #[rasn(tag(context, 4))]
    Reject(Reject),
    #[rasn(tag(context, 7))]
    ReturnResultNotLast(ReturnResult),
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invoke(inv) => write!(
                f,
                "Invoke [id={}, op={}]",
                inv.invoke_id, inv.operation_code
            ),
            Self::ReturnResultLast(rr) => write!(f, "ReturnResultLast [id={}]", rr.invoke_id),
            Self::ReturnResultNotLast(rr) => write!(f, "ReturnResultNotLast [id={}]", rr.invoke_id),
            Self::ReturnError(re) => write!(
                f,
                "ReturnError [id={}, err={}]",
                re.invoke_id, re.error_code
            ),
            Self::Reject(rj) => write!(f, "Reject [id={}]", rj.invoke_id),
        }
    }
}

/// Invoke component.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Invoke {
    pub invoke_id: InvokeId,
    #[rasn(tag(context, 0))]
    pub linked_id: Option<InvokeId>,
    pub operation_code: OperationCode,
    /// Opaque parameter — decoded by the application layer (e.g., MAP).
    pub parameter: Option<rasn::types::Any>,
}

/// Return Result component (used for both Last and NotLast).
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct ReturnResult {
    pub invoke_id: InvokeId,
    pub result: Option<ReturnResultValue>,
}

/// The value inside a ReturnResult.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct ReturnResultValue {
    pub operation_code: OperationCode,
    pub parameter: Option<rasn::types::Any>,
}

/// Return Error component.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct ReturnError {
    pub invoke_id: InvokeId,
    pub error_code: ErrorCode,
    pub parameter: Option<rasn::types::Any>,
}

/// Reject component.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Reject {
    pub invoke_id: InvokeId,
    pub problem: rasn::types::Any,
}
