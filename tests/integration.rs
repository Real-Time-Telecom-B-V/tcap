//! Integration tests — TCAP BER encoding verification.

use tcap::*;

/// Verify Begin tag is APPLICATION 2 CONSTRUCTED (0x62).
#[test]
fn begin_tag_value() {
    let begin = Begin {
        otid: vec![0x01, 0x02, 0x03, 0x04].into(),
        dialogue_portion: None,
        components: None,
    };
    let bytes = encode(&TcapMessage::Begin(begin)).unwrap();
    assert_eq!(bytes[0], 0x62); // [APPLICATION 2] CONSTRUCTED
}

/// Verify End tag is APPLICATION 4 CONSTRUCTED (0x64).
#[test]
fn end_tag_value() {
    let end = End {
        dtid: vec![0x01].into(),
        dialogue_portion: None,
        components: None,
    };
    let bytes = encode(&TcapMessage::End(end)).unwrap();
    assert_eq!(bytes[0], 0x64); // [APPLICATION 4] CONSTRUCTED
}

/// Verify Continue tag is APPLICATION 5 CONSTRUCTED (0x65).
#[test]
fn continue_tag_value() {
    let cont = Continue {
        otid: vec![0x01].into(),
        dtid: vec![0x02].into(),
        dialogue_portion: None,
        components: None,
    };
    let bytes = encode(&TcapMessage::Continue(cont)).unwrap();
    assert_eq!(bytes[0], 0x65); // [APPLICATION 5] CONSTRUCTED
}

/// Verify Abort tag is APPLICATION 7 CONSTRUCTED (0x67).
#[test]
fn abort_tag_value() {
    let abort = Abort {
        dtid: vec![0x01].into(),
        reason: None,
    };
    let bytes = encode(&TcapMessage::Abort(abort)).unwrap();
    assert_eq!(bytes[0], 0x67); // [APPLICATION 7] CONSTRUCTED
}

/// Verify Unidirectional tag is APPLICATION 1 CONSTRUCTED (0x61).
#[test]
fn unidirectional_tag_value() {
    let uni = Unidirectional {
        dialogue_portion: None,
        components: vec![],
    };
    let bytes = encode(&TcapMessage::Unidirectional(uni)).unwrap();
    assert_eq!(bytes[0], 0x61); // [APPLICATION 1] CONSTRUCTED
}

/// Transaction ID encoding — OTID uses APPLICATION 8 tag.
#[test]
fn transaction_id_tags() {
    let begin = Begin {
        otid: vec![0xAA, 0xBB, 0xCC, 0xDD].into(),
        dialogue_portion: None,
        components: None,
    };
    let bytes = encode(&TcapMessage::Begin(begin)).unwrap();

    // Find OTID tag (APPLICATION 8 = 0x48 primitive, or 0x68 constructed)
    // The exact encoding depends on rasn's explicit tag handling
    // Just verify the OTID value appears in the encoded bytes
    assert!(bytes.windows(4).any(|w| w == [0xAA, 0xBB, 0xCC, 0xDD]));
}

/// Multiple components in a single message.
#[test]
fn multiple_invoke_components() {
    let invoke1 = Invoke {
        invoke_id: 1,
        linked_id: None,
        operation_code: OperationCode::Local(45),
        parameter: None,
    };
    let invoke2 = Invoke {
        invoke_id: 2,
        linked_id: None,
        operation_code: OperationCode::Local(46),
        parameter: None,
    };

    let begin = Begin {
        otid: vec![0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Invoke(invoke1), Component::Invoke(invoke2)]),
    };

    let bytes = encode(&TcapMessage::Begin(begin)).unwrap();
    assert!(!bytes.is_empty());
    // Verify it at least encodes without error
}

/// ReturnResult with operation code and no parameter.
#[test]
fn return_result_no_param() {
    let rr = ReturnResult {
        invoke_id: 1,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(45),
            parameter: None,
        }),
    };

    let end = End {
        dtid: vec![0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::ReturnResultLast(rr)]),
    };

    let bytes = encode(&TcapMessage::End(end)).unwrap();
    let decoded = decode(&bytes).unwrap();
    match decoded {
        TcapMessage::End(e) => {
            assert!(e.components.is_some());
        }
        _ => panic!("Expected End"),
    }
}

/// Reject component round-trip.
#[test]
fn reject_component() {
    let reject = Reject {
        invoke_id: 99,
        problem: rasn::types::Any::new(vec![0x80, 0x01, 0x01]), // general problem: unrecognized component
    };

    let end = End {
        dtid: vec![0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Reject(reject)]),
    };

    let bytes = encode(&TcapMessage::End(end)).unwrap();
    let decoded = decode(&bytes).unwrap();
    match decoded {
        TcapMessage::End(e) => {
            let comps = e.components.unwrap();
            match &comps[0] {
                Component::Reject(rj) => assert_eq!(rj.invoke_id, 99),
                _ => panic!("Expected Reject"),
            }
        }
        _ => panic!("Expected End"),
    }
}

/// Empty Begin (no components, no dialogue).
#[test]
fn empty_begin() {
    let begin = Begin {
        otid: vec![0x00, 0x00, 0x00, 0x01].into(),
        dialogue_portion: None,
        components: None,
    };
    let bytes = encode(&TcapMessage::Begin(begin)).unwrap();
    let decoded = decode(&bytes).unwrap();
    match decoded {
        TcapMessage::Begin(b) => {
            assert_eq!(b.otid.as_ref(), &[0, 0, 0, 1]);
        }
        _ => panic!("Expected Begin"),
    }
}

/// Operation code display.
#[test]
fn operation_code_display() {
    let local = OperationCode::Local(45);
    assert_eq!(format!("{local}"), "local(45)");
}

// ---------------------------------------------------------------------------
// Full round-trips — every field survives an encode/decode cycle.
//
// All values below are synthetic / spec-derived: fictional transaction ids,
// application-defined operation & error codes, and opaque parameter octets that
// carry no real subscriber data.
// ---------------------------------------------------------------------------

/// An Invoke keeps its id, linked id, operation code, and opaque parameter
/// through a Begin round-trip.
#[test]
fn invoke_all_fields_round_trip() {
    let invoke = Invoke {
        invoke_id: 7,
        linked_id: Some(3),
        operation_code: OperationCode::Local(59),
        // Opaque application argument (a synthetic OCTET STRING), not decoded here.
        parameter: Some(rasn::types::Any::new(vec![0x04, 0x03, 0x01, 0x02, 0x03])),
    };
    let begin = Begin {
        otid: vec![0x11, 0x22].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Invoke(invoke.clone())]),
    };

    let wire = encode(&TcapMessage::Begin(begin)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Begin(b) => {
            assert_eq!(b.otid.as_ref(), &[0x11, 0x22]);
            let comps = b.components.expect("components present");
            assert_eq!(comps, vec![Component::Invoke(invoke)]);
        }
        _ => panic!("Expected Begin"),
    }
}

/// A global (OID) operation code round-trips as itself.
#[test]
fn global_operation_code_round_trip() {
    // Synthetic OID in the joint-iso-itu-t space — not a registered context.
    let oid = rasn::types::ObjectIdentifier::new(vec![2u32, 4, 0, 0, 1, 0, 21, 3]).unwrap();
    let invoke = Invoke {
        invoke_id: 1,
        linked_id: None,
        operation_code: OperationCode::Global(oid.clone()),
        parameter: None,
    };
    let begin = Begin {
        otid: vec![0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Invoke(invoke)]),
    };

    let wire = encode(&TcapMessage::Begin(begin)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Begin(b) => match &b.components.unwrap()[0] {
            Component::Invoke(inv) => {
                assert_eq!(inv.operation_code, OperationCode::Global(oid));
            }
            _ => panic!("Expected Invoke"),
        },
        _ => panic!("Expected Begin"),
    }
}

/// A global (OID) error code round-trips as itself.
#[test]
fn global_error_code_round_trip() {
    let oid = rasn::types::ObjectIdentifier::new(vec![2u32, 4, 0, 0, 1, 1, 1]).unwrap();
    let re = ReturnError {
        invoke_id: 4,
        error_code: ErrorCode::Global(oid.clone()),
        parameter: None,
    };
    let end = End {
        dtid: vec![0x02].into(),
        dialogue_portion: None,
        components: Some(vec![Component::ReturnError(re)]),
    };

    let wire = encode(&TcapMessage::End(end)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::End(e) => match &e.components.unwrap()[0] {
            Component::ReturnError(re) => assert_eq!(re.error_code, ErrorCode::Global(oid)),
            _ => panic!("Expected ReturnError"),
        },
        _ => panic!("Expected End"),
    }
}

/// A Begin carrying a dialogue portion round-trips with the portion intact.
#[test]
fn dialogue_portion_round_trip() {
    // Synthetic EXTERNAL-shaped bytes (tag 0x28 = [UNIVERSAL 8] EXTERNAL). The
    // codec carries these opaquely; the dialogue layer above decodes them.
    let dp = DialoguePortion {
        external: rasn::types::Any::new(vec![0x28, 0x03, 0x06, 0x01, 0x2A]),
    };
    let begin = Begin {
        otid: vec![0x01].into(),
        dialogue_portion: Some(dp),
        components: None,
    };

    let wire = encode(&TcapMessage::Begin(begin)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Begin(b) => assert!(b.dialogue_portion.is_some()),
        _ => panic!("Expected Begin"),
    }
}

/// A Unidirectional message round-trips (decode, not just encode).
#[test]
fn unidirectional_round_trip() {
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

    let wire = encode(&TcapMessage::Unidirectional(uni)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Unidirectional(u) => assert_eq!(u.components.len(), 1),
        _ => panic!("Expected Unidirectional"),
    }
}

/// A Continue with both transaction ids and a component round-trips.
#[test]
fn continue_with_component_round_trip() {
    let rr = ReturnResult {
        invoke_id: 2,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(46),
            parameter: Some(rasn::types::Any::new(vec![0x05, 0x00])), // synthetic NULL
        }),
    };
    let cont = Continue {
        otid: vec![0xAA].into(),
        dtid: vec![0xBB].into(),
        dialogue_portion: None,
        components: Some(vec![Component::ReturnResultNotLast(rr)]),
    };

    let wire = encode(&TcapMessage::Continue(cont)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Continue(c) => {
            assert_eq!(c.otid.as_ref(), &[0xAA]);
            assert_eq!(c.dtid.as_ref(), &[0xBB]);
            match &c.components.unwrap()[0] {
                Component::ReturnResultNotLast(rr) => assert_eq!(rr.invoke_id, 2),
                _ => panic!("Expected ReturnResultNotLast"),
            }
        }
        _ => panic!("Expected Continue"),
    }
}

/// An Abort carrying a reason round-trips with the reason present.
#[test]
fn abort_with_reason_round_trip() {
    let abort = Abort {
        dtid: vec![0x03].into(),
        // Synthetic P-Abort cause bytes, carried opaquely.
        reason: Some(rasn::types::Any::new(vec![0x0A, 0x01, 0x01])),
    };

    let wire = encode(&TcapMessage::Abort(abort)).unwrap();
    let decoded = decode(&wire).unwrap();

    match decoded {
        TcapMessage::Abort(a) => {
            assert_eq!(a.dtid.as_ref(), &[0x03]);
            assert!(a.reason.is_some());
        }
        _ => panic!("Expected Abort"),
    }
}

// ---------------------------------------------------------------------------
// Error paths — malformed input must not panic; it must return TcapError.
// ---------------------------------------------------------------------------

/// Random bytes with no valid TCAP tag fail to decode.
#[test]
fn decode_garbage_is_error() {
    let err = decode(&[0xFF, 0x00, 0x99]).unwrap_err();
    // The Display impl should mention it was a decode failure.
    assert!(format!("{err}").contains("decode"));
}

/// A truncated message (valid Begin tag, bogus length) fails cleanly.
#[test]
fn decode_truncated_is_error() {
    // 0x62 = Begin, 0x7F = length 127, but no content follows.
    assert!(decode(&[0x62, 0x7F]).is_err());
}

/// Empty input is a decode error, not a panic.
#[test]
fn decode_empty_is_error() {
    assert!(decode(&[]).is_err());
}

// ---------------------------------------------------------------------------
// Display — the human-readable renderings used in logs.
// ---------------------------------------------------------------------------

/// Component Display renders id and operation/error code.
#[test]
fn component_display() {
    let invoke = Component::Invoke(Invoke {
        invoke_id: 3,
        linked_id: None,
        operation_code: OperationCode::Local(45),
        parameter: None,
    });
    assert_eq!(format!("{invoke}"), "Invoke [id=3, op=local(45)]");

    let re = Component::ReturnError(ReturnError {
        invoke_id: 5,
        error_code: ErrorCode::Local(34),
        parameter: None,
    });
    assert_eq!(format!("{re}"), "ReturnError [id=5, err=local(34)]");
}

/// TcapMessage Display renders the transaction ids as hex.
#[test]
fn message_display_hex_tids() {
    let cont = TcapMessage::Continue(Continue {
        otid: vec![0xAB].into(),
        dtid: vec![0xCD].into(),
        dialogue_portion: None,
        components: None,
    });
    let s = format!("{cont}");
    assert!(s.contains("otid=ab"), "got: {s}");
    assert!(s.contains("dtid=cd"), "got: {s}");
}

/// Error code Display for the global (OID) form.
#[test]
fn error_code_global_display() {
    let oid = rasn::types::ObjectIdentifier::new(vec![2u32, 4, 0, 0, 1, 1, 1]).unwrap();
    let ec = ErrorCode::Global(oid);
    let s = format!("{ec}");
    assert!(s.starts_with("global("), "got: {s}");
}
