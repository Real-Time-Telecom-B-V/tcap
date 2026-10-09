use std::fmt;

use rasn::prelude::*;

use crate::types::{
    ErrorCode, GeneralProblem, InvokeId, InvokeProblem, OperationCode, ReturnErrorProblem,
    ReturnResultProblem,
};

/// TCAP Component types per ITU-T Q.773 clause 3.1.
///
/// The derived `rasn::Decode` is what the crate's decoder runs on one
/// component at a time, followed by a check that nothing on the wire was
/// dropped. Calling `rasn::ber::decode` on these types directly skips that
/// check; use [`crate::decode`] or [`crate::decode_detailed`].
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

/// The component type, which is the context tag of a [`Component`]
/// (Table 19/Q.773).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentType {
    /// `[1]`
    Invoke,
    /// `[2]`
    ReturnResultLast,
    /// `[3]`
    ReturnError,
    /// `[4]`
    Reject,
    /// `[7]`
    ReturnResultNotLast,
}

impl ComponentType {
    /// The context tag number.
    pub fn tag(self) -> u8 {
        match self {
            Self::Invoke => 1,
            Self::ReturnResultLast => 2,
            Self::ReturnError => 3,
            Self::Reject => 4,
            Self::ReturnResultNotLast => 7,
        }
    }

    /// The component type for a context tag number, if Q.773 defines one.
    pub fn from_tag(tag: u32) -> Option<Self> {
        match tag {
            1 => Some(Self::Invoke),
            2 => Some(Self::ReturnResultLast),
            3 => Some(Self::ReturnError),
            4 => Some(Self::Reject),
            7 => Some(Self::ReturnResultNotLast),
            _ => None,
        }
    }
}

impl fmt::Display for ComponentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invoke => "Invoke",
            Self::ReturnResultLast => "ReturnResultLast",
            Self::ReturnError => "ReturnError",
            Self::Reject => "Reject",
            Self::ReturnResultNotLast => "ReturnResultNotLast",
        })
    }
}

impl Component {
    /// The component type.
    pub fn component_type(&self) -> ComponentType {
        match self {
            Self::Invoke(_) => ComponentType::Invoke,
            Self::ReturnResultLast(_) => ComponentType::ReturnResultLast,
            Self::ReturnError(_) => ComponentType::ReturnError,
            Self::Reject(_) => ComponentType::Reject,
            Self::ReturnResultNotLast(_) => ComponentType::ReturnResultNotLast,
        }
    }

    /// The invoke ID the component carries. `None` only for a Reject whose
    /// invoke ID is the not-derivable NULL.
    pub fn invoke_id(&self) -> Option<InvokeId> {
        match self {
            Self::Invoke(invoke) => Some(invoke.invoke_id),
            Self::ReturnResultLast(result) | Self::ReturnResultNotLast(result) => {
                Some(result.invoke_id)
            }
            Self::ReturnError(error) => Some(error.invoke_id),
            Self::Reject(reject) => reject.invoke_id,
        }
    }
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
            Self::Reject(rj) => match rj.invoke_id {
                Some(id) => write!(f, "Reject [id={id}, problem={}]", rj.problem),
                None => write!(f, "Reject [id not derivable, problem={}]", rj.problem),
            },
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
///
/// Q.773 gives `result SEQUENCE { operationCode OPERATION, parameter ANY
/// DEFINED BY operationCode }`: when the sequence is present, so is the
/// parameter. [`crate::encode`] therefore refuses a value with `parameter:
/// None` (leave the whole `result` out for an operation that returns nothing).
/// The decoder accepts the operation code alone, which some deployed
/// implementations send and which loses no information; that is why the
/// member is an `Option`.
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

/// The problem code of a Reject component: one of four classes, each with its
/// own values (3.1/Q.773, Tables 25 to 29/Q.773).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum Problem {
    /// `generalProblem [0]`: raised by the component sub-layer for a component
    /// it cannot understand.
    #[rasn(tag(context, 0))]
    General(GeneralProblem),
    /// `invokeProblem [1]`
    #[rasn(tag(context, 1))]
    Invoke(InvokeProblem),
    /// `returnResultProblem [2]`
    #[rasn(tag(context, 2))]
    ReturnResult(ReturnResultProblem),
    /// `returnErrorProblem [3]`
    #[rasn(tag(context, 3))]
    ReturnError(ReturnErrorProblem),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::General(problem) => write!(f, "general:{problem}"),
            Self::Invoke(problem) => write!(f, "invoke:{problem}"),
            Self::ReturnResult(problem) => write!(f, "returnResult:{problem}"),
            Self::ReturnError(problem) => write!(f, "returnError:{problem}"),
        }
    }
}

/// Reject component.
///
/// ```text
/// Reject ::= SEQUENCE {
///     invokeID CHOICE { derivable InvokeIdType, not-derivable NULL },
///     problem  CHOICE { generalProblem [0] .. returnErrorProblem [3] } }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reject {
    /// The invoke ID of the rejected component, or `None` when it could not
    /// be derived, which is sent as a NULL (Table 18/Q.773, note a).
    pub invoke_id: Option<InvokeId>,
    pub problem: Problem,
}

impl Reject {
    /// A Reject with a general problem, as the component sub-layer builds it
    /// for a component it could not understand (3.2.2.2/Q.774: "When an invoke
    /// ID is available in a component to be rejected, this ID is reflected in
    /// the Reject component").
    pub fn general(invoke_id: Option<InvokeId>, problem: GeneralProblem) -> Self {
        Self {
            invoke_id,
            problem: Problem::General(problem),
        }
    }

    /// A Reject of an Invoke, by the component sub-layer or the TC-user.
    pub fn invoke(invoke_id: InvokeId, problem: InvokeProblem) -> Self {
        Self {
            invoke_id: Some(invoke_id),
            problem: Problem::Invoke(problem),
        }
    }

    /// A Reject of a ReturnResult.
    pub fn return_result(invoke_id: InvokeId, problem: ReturnResultProblem) -> Self {
        Self {
            invoke_id: Some(invoke_id),
            problem: Problem::ReturnResult(problem),
        }
    }

    /// A Reject of a ReturnError.
    pub fn return_error(invoke_id: InvokeId, problem: ReturnErrorProblem) -> Self {
        Self {
            invoke_id: Some(invoke_id),
            problem: Problem::ReturnError(problem),
        }
    }
}

// The wire form of a Reject. The public struct holds the invoke ID as an
// `Option`, which the derive would read as OPTIONAL; on the wire it is a
// CHOICE between an INTEGER and a NULL, and one of the two is always there.
#[derive(rasn::AsnType, rasn::Decode, rasn::Encode)]
struct RejectWire {
    invoke_id: RejectInvokeId,
    problem: Problem,
}

#[derive(rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
enum RejectInvokeId {
    Derivable(InvokeId),
    NotDerivable(()),
}

impl AsnType for Reject {
    const TAG: Tag = RejectWire::TAG;
}

impl Encode for Reject {
    fn encode_with_tag_and_constraints<'encoder, E: Encoder<'encoder>>(
        &self,
        encoder: &mut E,
        tag: Tag,
        constraints: Constraints,
        identifier: Identifier,
    ) -> Result<(), E::Error> {
        RejectWire {
            invoke_id: match self.invoke_id {
                Some(id) => RejectInvokeId::Derivable(id),
                None => RejectInvokeId::NotDerivable(()),
            },
            problem: self.problem,
        }
        .encode_with_tag_and_constraints(encoder, tag, constraints, identifier)
    }
}

impl Decode for Reject {
    fn decode_with_tag_and_constraints<D: Decoder>(
        decoder: &mut D,
        tag: Tag,
        constraints: Constraints,
    ) -> Result<Self, D::Error> {
        let wire = RejectWire::decode_with_tag_and_constraints(decoder, tag, constraints)?;
        Ok(Self {
            invoke_id: match wire.invoke_id {
                RejectInvokeId::Derivable(id) => Some(id),
                RejectInvokeId::NotDerivable(()) => None,
            },
            problem: wire.problem,
        })
    }
}
