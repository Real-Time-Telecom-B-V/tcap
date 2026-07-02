//! TCAP (Transaction Capabilities Application Part) codec per ITU-T Q.771-Q.775.
//!
//! Provides BER encode/decode for TCAP messages including:
//! - Transaction types: Begin, Continue, End, Abort, Unidirectional
//! - Component types: Invoke, ReturnResult, ReturnError, Reject
//! - Dialogue portion for application context negotiation
//!
//! Uses `rasn` for ASN.1 BER encoding/decoding.
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

pub mod component;
pub mod dialogue;
pub mod error;
pub mod transaction;
pub mod types;

pub use component::{Component, Invoke, Reject, ReturnError, ReturnResult, ReturnResultValue};
pub use dialogue::DialoguePortion;
pub use error::TcapError;
pub use transaction::{Abort, Begin, Continue, End, TcapMessage, Unidirectional};
pub use types::{ErrorCode, InvokeId, OperationCode};

/// Encode a TCAP message to wire-correct BER bytes.
///
/// With implicit APPLICATION tags, rasn directly produces Q.773-compliant encoding.
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError> {
    let raw = rasn::ber::encode(msg)?;
    Ok(raw)
}

/// Decode a TCAP message from wire-correct BER bytes.
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError> {
    let msg = rasn::ber::decode::<TcapMessage>(bytes)?;
    Ok(msg)
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
                parameter: None,
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
