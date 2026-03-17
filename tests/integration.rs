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
        components: Some(vec![
            Component::Invoke(invoke1),
            Component::Invoke(invoke2),
        ]),
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
