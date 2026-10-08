//! TCAP (Transaction Capabilities Application Part) codec per ITU-T Q.771-Q.775.
//!
//! Provides BER encode/decode for TCAP messages including:
//! - Transaction types: Begin, Continue, End, Abort, Unidirectional
//! - Component types: Invoke, ReturnResult, ReturnError, Reject
//! - Dialogue portion for application context negotiation
//!
//! Uses `rasn` for ASN.1 BER encoding/decoding of the components.
//!
//! # Decoding never loses anything silently
//!
//! [`decode`] returns a message only when every part of it was understood and
//! every octet of the input belongs to it. Otherwise it fails with
//! [`TcapError::Malformed`], which carries a [`DecodeProblem`]: which
//! sub-layer detected the problem, the P-Abort cause or the general problem
//! code, the transaction IDs and the invoke ID that could be recovered, and,
//! for a problem in a component, the message up to that component.
//! [`DecodeProblem::abort`] and [`DecodeProblem::reject`] build the response
//! Q.774 requires. [`decode_detailed`] gives the same as a plain value.
//!
//! ```
//! use tcap::{Decoded, GeneralProblem, Reject};
//!
//! // A Begin whose only Invoke stops after the invoke ID.
//! let wire = [
//!     0x62, 0x0d, 0x48, 0x04, 0x00, 0x00, 0x10, 0x01, 0x6c, 0x05, 0xa1, 0x03, 0x02, 0x01, 0x02,
//! ];
//! let Decoded::Problem(problem) = tcap::decode_detailed(&wire) else {
//!     panic!("this message is not valid");
//! };
//! assert_eq!(
//!     problem.reject(),
//!     Some(Reject::general(Some(2), GeneralProblem::MISTYPED_COMPONENT))
//! );
//! assert!(tcap::decode(&wire).is_err());
//! ```
//!
//! # Example
//!
//! ```
//! use tcap::{TcapMessage, Begin, Component, Invoke, OperationCode};
//!
//! // Build a TCAP Begin with an Invoke component
//! let invoke = Invoke {
//!     invoke_id: 1,
//!     linked_id: None,
//!     operation_code: OperationCode::Local(45), // an application-defined operation
//!     parameter: None,                          // MAP/CAP/INAP argument goes here
//! };
//!
//! let begin = Begin {
//!     otid: vec![0x00, 0x00, 0x00, 0x01].into(),
//!     dialogue_portion: None,
//!     components: Some(vec![Component::Invoke(invoke)]),
//! };
//!
//! let msg = TcapMessage::Begin(begin);
//! let encoded = tcap::encode(&msg).unwrap();
//! let decoded = tcap::decode(&encoded).unwrap();
//! ```

mod ber;
pub mod component;
mod decode;
pub mod dialogue;
pub mod error;
pub mod transaction;
pub mod types;

#[cfg(feature = "python")]
pub mod python;

pub use component::{
    Component, ComponentType, Invoke, Problem, Reject, ReturnError, ReturnResult, ReturnResultValue,
};
pub use decode::{decode_detailed, DecodeProblem, Decoded, Fault, Sublayer};
pub use dialogue::{
    AbortSource, ApplicationContextName, AssociateResult, AssociateSourceDiagnostic,
    DialogueContent, DialogueError, DialoguePdu, DialoguePortion, External, ExternalEncoding,
    ProtocolVersion,
};
pub use error::TcapError;
pub use transaction::{
    Abort, AbortReason, Begin, Continue, End, MessageType, TcapMessage, Unidirectional,
};
pub use types::{
    ErrorCode, GeneralProblem, InvokeId, InvokeProblem, OperationCode, PAbortCause,
    ReturnErrorProblem, ReturnResultProblem, TransactionId,
};

/// Encode a TCAP message to BER as Q.773 specifies it.
///
/// A message Q.773 does not allow is refused with
/// [`TcapError::InvalidMessage`] instead of being put on the wire:
///
/// * a transaction ID that is not 1 to 4 octets long (`SIZE (1..4)`);
/// * a component portion with no components (`SIZE (1..MAX)`): leave
///   `components` as `None` for a message without components;
/// * a ReturnResult whose `result` has no parameter (the parameter of the
///   result sequence is not OPTIONAL): leave `result` as `None`;
/// * a parameter that is not exactly one BER element;
/// * a dialogue portion that is not a well-formed `EXTERNAL`, or that names a
///   Q.773 dialogue abstract syntax and does not hold a well-formed PDU.
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError> {
    validate(msg).map_err(TcapError::InvalidMessage)?;
    let raw = rasn::ber::encode(msg)?;
    Ok(raw)
}

/// Decode one TCAP message that has to be fully valid.
///
/// `bytes` is the user data of one SCCP message and must be exactly one TCAP
/// message. Any part that cannot be understood makes the whole call fail with
/// [`TcapError::Malformed`]: a message is never returned without a component,
/// a dialogue portion or trailing octets that were on the wire. The error
/// carries the [`DecodeProblem`], from which the Abort or Reject that Q.774
/// requires is built. [`decode_detailed`] returns the same information
/// without going through an error.
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError> {
    decode_detailed(bytes)
        .into_result()
        .map_err(TcapError::Malformed)
}

fn validate(msg: &TcapMessage) -> Result<(), String> {
    for (name, id) in [("originating", msg.otid()), ("destination", msg.dtid())] {
        if let Some(id) = id {
            if !(1..=4).contains(&id.len()) {
                return Err(format!(
                    "{name} transaction ID is {} octets long, Q.773 allows 1 to 4",
                    id.len()
                ));
            }
        }
    }

    let empty_portion = match msg {
        TcapMessage::Unidirectional(uni) => uni.components.is_empty(),
        TcapMessage::Begin(Begin { components, .. })
        | TcapMessage::End(End { components, .. })
        | TcapMessage::Continue(Continue { components, .. }) => {
            components.as_ref().is_some_and(Vec::is_empty)
        }
        TcapMessage::Abort(_) => false,
    };
    if empty_portion {
        return Err("a component portion holds at least one component".to_string());
    }

    for (index, component) in msg.components().iter().enumerate() {
        let parameter = match component {
            Component::Invoke(invoke) => invoke.parameter.as_ref(),
            Component::ReturnError(error) => error.parameter.as_ref(),
            Component::ReturnResultLast(result) | Component::ReturnResultNotLast(result) => {
                match &result.result {
                    Some(value) => Some(value.parameter.as_ref().ok_or_else(|| {
                        format!(
                            "component {index}: the result of a ReturnResult has no parameter, \
                             leave the result out instead"
                        )
                    })?),
                    None => None,
                }
            }
            Component::Reject(_) => None,
        };
        if let Some(parameter) = parameter {
            let sole = ber::element(parameter.as_bytes())
                .map_err(|e| format!("component {index}: parameter: {e}"))?;
            if !sole.rest.is_empty() {
                return Err(format!(
                    "component {index}: the parameter is more than one element"
                ));
            }
        }
    }

    if let Some(portion) = msg.dialogue_portion() {
        portion.parse().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_no_components_round_trip() {
        let begin = Begin {
            otid: vec![0x00, 0x00, 0x00, 0x01].into(),
            dialogue_portion: None,
            components: None,
        };

        let msg = TcapMessage::Begin(begin);
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            TcapMessage::Begin(b) => {
                assert_eq!(b.otid.as_ref(), &[0, 0, 0, 1]);
            }
            _ => panic!("Expected Begin"),
        }
    }

    #[test]
    fn begin_with_invoke() {
        let invoke = Invoke {
            invoke_id: 1,
            linked_id: None,
            operation_code: OperationCode::Local(45),
            parameter: None,
        };

        let begin = Begin {
            otid: vec![0x00, 0x00, 0x00, 0x01].into(),
            dialogue_portion: None,
            components: Some(vec![Component::Invoke(invoke)]),
        };

        let msg = TcapMessage::Begin(begin);
        let encoded = encode(&msg).unwrap();

        // Verify encoding produces bytes (BER encoding test)
        assert!(!encoded.is_empty());

        // Verify Begin tag (0x62 = APPLICATION 2, CONSTRUCTED)
        assert_eq!(encoded[0], 0x62);
    }

    #[test]
    fn end_return_result_round_trip() {
        let rr = ReturnResult {
            invoke_id: 1,
            result: Some(ReturnResultValue {
                operation_code: OperationCode::Local(45),
                // An empty SEQUENCE as the result parameter.
                parameter: Some(rasn::types::Any::new(vec![0x30, 0x00])),
            }),
        };

        let end = End {
            dtid: vec![0x00, 0x00, 0x00, 0x02].into(),
            dialogue_portion: None,
            components: Some(vec![Component::ReturnResultLast(rr)]),
        };

        let msg = TcapMessage::End(end);
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            TcapMessage::End(e) => {
                assert_eq!(e.dtid.as_ref(), &[0, 0, 0, 2]);
                let components = e.components.unwrap();
                assert_eq!(components.len(), 1);
            }
            _ => panic!("Expected End"),
        }
    }

    #[test]
    fn continue_round_trip() {
        let cont = Continue {
            otid: vec![0x01].into(),
            dtid: vec![0x02].into(),
            dialogue_portion: None,
            components: None,
        };

        let msg = TcapMessage::Continue(cont);
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            TcapMessage::Continue(c) => {
                assert_eq!(c.otid.as_ref(), &[1]);
                assert_eq!(c.dtid.as_ref(), &[2]);
            }
            _ => panic!("Expected Continue"),
        }
    }

    #[test]
    fn abort_round_trip() {
        let abort = Abort {
            dtid: vec![0x00, 0x00, 0x00, 0x03].into(),
            reason: None,
        };

        let msg = TcapMessage::Abort(abort);
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            TcapMessage::Abort(a) => {
                assert_eq!(a.dtid.as_ref(), &[0, 0, 0, 3]);
            }
            _ => panic!("Expected Abort"),
        }
    }

    #[test]
    fn unidirectional_encoding() {
        let invoke = Invoke {
            invoke_id: 0,
            linked_id: None,
            operation_code: OperationCode::Local(59),
            parameter: None,
        };

        let uni = Unidirectional {
            dialogue_portion: None,
            components: vec![Component::Invoke(invoke)],
        };

        let msg = TcapMessage::Unidirectional(uni);
        let encoded = encode(&msg).unwrap();

        // Verify Unidirectional tag (0x61 = APPLICATION 1, CONSTRUCTED)
        assert_eq!(encoded[0], 0x61);
        assert!(!encoded.is_empty());
    }

    #[test]
    fn return_error_round_trip() {
        let re = ReturnError {
            invoke_id: 5,
            error_code: ErrorCode::Local(34), // systemFailure
            parameter: None,
        };

        let end = End {
            dtid: vec![0x01].into(),
            dialogue_portion: None,
            components: Some(vec![Component::ReturnError(re)]),
        };

        let msg = TcapMessage::End(end);
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            TcapMessage::End(e) => {
                let comps = e.components.unwrap();
                match &comps[0] {
                    Component::ReturnError(re) => {
                        assert_eq!(re.invoke_id, 5);
                        assert_eq!(re.error_code, ErrorCode::Local(34));
                    }
                    _ => panic!("Expected ReturnError"),
                }
            }
            _ => panic!("Expected End"),
        }
    }

    #[test]
    fn display() {
        let msg = TcapMessage::Begin(Begin {
            otid: vec![0x01, 0x02].into(),
            dialogue_portion: None,
            components: Some(vec![]),
        });
        let s = format!("{msg}");
        assert!(s.contains("Begin"));
    }
}
