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
//!     operation_code: OperationCode::Local(45), // sendRoutingInfoForSM
//!     parameter: None,
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
/// `rasn` encodes struct-based choice variants as `[APPLICATION N] { SEQUENCE { fields } }`,
/// but ITU-T Q.773 uses implicit tagging where the APPLICATION tag replaces SEQUENCE:
/// `[APPLICATION N] { fields }`. This function strips the redundant inner SEQUENCE.
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError> {
    let raw = rasn::ber::encode(msg)?;
    Ok(strip_inner_sequence(&raw))
}

/// Decode a TCAP message from wire-correct BER bytes.
///
/// Re-inserts the inner SEQUENCE wrapper that `rasn` expects before decoding.
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError> {
    let wrapped = insert_inner_sequence(bytes);
    let msg = rasn::ber::decode::<TcapMessage>(&wrapped)?;
    Ok(msg)
}

/// Strip the inner SEQUENCE (0x30) that rasn adds inside the APPLICATION tag.
///
/// Input:  `62 LL 30 LL' <fields>`  (rasn output)
/// Output: `62 LL' <fields>`        (wire-correct Q.773)
fn strip_inner_sequence(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() < 4 {
        return bytes.to_vec();
    }

    // Parse outer tag + length
    let outer_tag = bytes[0];
    let (outer_len_size, _outer_len) = parse_ber_length(&bytes[1..]);
    let content_start = 1 + outer_len_size;

    // Check if content starts with SEQUENCE (0x30)
    if content_start < bytes.len() && bytes[content_start] == 0x30 {
        let (seq_len_size, seq_len) = parse_ber_length(&bytes[content_start + 1..]);
        let fields_start = content_start + 1 + seq_len_size;

        // Rebuild: outer_tag + new_length + fields (without the SEQUENCE wrapper)
        let fields = &bytes[fields_start..fields_start + seq_len];
        let mut result = Vec::with_capacity(1 + 4 + fields.len());
        result.push(outer_tag);
        encode_ber_length(&mut result, fields.len());
        result.extend_from_slice(fields);
        result
    } else {
        bytes.to_vec()
    }
}

/// Insert an inner SEQUENCE (0x30) wrapper inside the APPLICATION tag.
///
/// Input:  `62 LL' <fields>`        (wire Q.773)
/// Output: `62 LL 30 LL' <fields>`  (what rasn expects)
fn insert_inner_sequence(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() < 2 {
        return bytes.to_vec();
    }

    let outer_tag = bytes[0];
    let (outer_len_size, outer_len) = parse_ber_length(&bytes[1..]);
    let content_start = 1 + outer_len_size;

    // Check if content already starts with SEQUENCE — if so, no wrapping needed
    if content_start < bytes.len() && bytes[content_start] == 0x30 {
        return bytes.to_vec();
    }

    let content = &bytes[content_start..content_start + outer_len];

    // Build SEQUENCE wrapper around content
    let mut seq = vec![0x30];
    encode_ber_length(&mut seq, content.len());
    seq.extend_from_slice(content);

    // Rebuild outer
    let mut result = Vec::with_capacity(1 + 4 + seq.len());
    result.push(outer_tag);
    encode_ber_length(&mut result, seq.len());
    result.extend_from_slice(&seq);
    result
}

/// Parse a BER length field. Returns (bytes consumed, length value).
fn parse_ber_length(bytes: &[u8]) -> (usize, usize) {
    if bytes.is_empty() {
        return (0, 0);
    }
    if bytes[0] & 0x80 == 0 {
        // Short form
        (1, bytes[0] as usize)
    } else {
        let num_bytes = (bytes[0] & 0x7F) as usize;
        let mut len = 0usize;
        for i in 0..num_bytes {
            if 1 + i < bytes.len() {
                len = (len << 8) | (bytes[1 + i] as usize);
            }
        }
        (1 + num_bytes, len)
    }
}

/// Encode a BER length field.
fn encode_ber_length(buf: &mut Vec<u8>, len: usize) {
    if len < 128 {
        buf.push(len as u8);
    } else if len < 256 {
        buf.push(0x81);
        buf.push(len as u8);
    } else {
        buf.push(0x82);
        buf.push((len >> 8) as u8);
        buf.push((len & 0xFF) as u8);
    }
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
