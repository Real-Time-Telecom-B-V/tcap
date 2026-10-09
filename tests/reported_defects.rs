//! The four defects reported against 1.0.0, each reproduced and each closed.
//!
//! Every test first shows what `rasn` does on its own with this crate's
//! types: the decode succeeds and something that was on the wire is gone
//! (seen on rasn 0.28.14 and 0.28.15). That half documents the library
//! behaviour; if a future `rasn` stops being lenient it fails and says so.
//! The second half is what this crate guarantees either way: the message is
//! not returned as valid, and the caller gets what Q.774 needs for the
//! response.
//!
//! All values are synthetic.

mod common;

use common::{dissect_unchecked, problem, vector};
use tcap::{
    Abort, Component, ComponentType, DialogueContent, DialoguePortion, Fault, GeneralProblem,
    Invoke, MessageType, OperationCode, PAbortCause, Reject, Sublayer, TcapMessage,
};

/// `rasn` alone, without this crate's decoder.
fn lenient(bytes: &[u8]) -> TcapMessage {
    rasn::ber::decode(bytes).expect(
        "rasn refused these octets: it no longer has the leniency this test documents, \
         which is good news; adjust the first half of the test",
    )
}

fn invoke_58(invoke_id: i8) -> Component {
    Component::Invoke(Invoke {
        invoke_id,
        linked_id: None,
        operation_code: OperationCode::Local(58),
        parameter: None,
    })
}

// ── 1. A component portion whose only component is malformed ────────────────

#[test]
fn defect_1_a_lone_malformed_invoke_is_not_an_empty_component_list() {
    // The message from the report. The Invoke holds an invoke ID and then
    // ends: the operation code, which is mandatory, is missing.
    let wire = vector(
        "62 0d                      -- Begin, 13 octets
            48 04 00 00 10 01       -- otid
            6c 05                   -- ComponentPortion
               a1 03                -- invoke
                  02 01 02          -- invokeID 2, and nothing else",
    );

    match lenient(&wire) {
        TcapMessage::Begin(begin) => assert_eq!(
            begin.components,
            Some(vec![]),
            "rasn returns an empty list for a portion that held a component"
        ),
        other => panic!("{other:?}"),
    }

    let found = problem(&wire);
    assert_eq!(found.sublayer(), Sublayer::Component);
    assert_eq!(
        found.fault,
        Fault::Component {
            index: 0,
            component_type: Some(ComponentType::Invoke),
            invoke_id: Some(2),
            problem: GeneralProblem::MISTYPED_COMPONENT,
        }
    );
    assert_eq!(found.message_type, Some(MessageType::Begin));
    assert_eq!(found.otid.as_deref(), Some(&[0x00, 0x00, 0x10, 0x01][..]));
    // The Reject Q.774 asks for carries the invoke ID that could be read.
    assert_eq!(
        found.reject(),
        Some(Reject::general(Some(2), GeneralProblem::MISTYPED_COMPONENT))
    );
    assert_eq!(found.abort(), None, "a component fault is not an abort");
    // Nothing came before the faulty component.
    assert_eq!(
        found.partial.as_ref().map(|m| m.components().len()),
        Some(0)
    );
}

// ── 2. A good component followed by a malformed last one ───────────────────

#[test]
fn defect_2_a_malformed_last_invoke_is_not_dropped() {
    let wire = vector(
        "62 15                      -- Begin, 6 + 15 = 21
            48 04 00 00 10 01
            6c 0d                   -- ComponentPortion, 8 + 5 = 13
               a1 06 02 01 01 02 01 3a   -- invoke 1, operation 58
               a1 03 02 01 02            -- invoke 2 without an operation code",
    );

    assert_eq!(
        lenient(&wire).components(),
        &[invoke_58(1)],
        "rasn returns the first component and says nothing of the second"
    );

    let found = problem(&wire);
    assert_eq!(
        found.fault,
        Fault::Component {
            index: 1,
            component_type: Some(ComponentType::Invoke),
            invoke_id: Some(2),
            problem: GeneralProblem::MISTYPED_COMPONENT,
        }
    );
    // The component before the faulty one stands and is handed over.
    assert_eq!(
        found.partial.as_ref().map(TcapMessage::components),
        Some(&[invoke_58(1)][..])
    );
    assert_eq!(
        found.reject(),
        Some(Reject::general(Some(2), GeneralProblem::MISTYPED_COMPONENT))
    );
}

#[test]
fn defect_2_a_last_return_result_with_a_malformed_result_is_not_dropped() {
    // The result SEQUENCE holds a NULL where the operation code belongs.
    let wire = vector(
        "62 19                      -- Begin, 6 + 19 = 25
            48 04 00 00 10 01
            6c 11                   -- ComponentPortion, 8 + 9 = 17
               a1 06 02 01 01 02 01 3a   -- invoke 1, operation 58
               a2 07                     -- returnResultLast
                  02 01 02               -- invokeID 2
                  30 02 05 00            -- result SEQUENCE { NULL }",
    );

    assert_eq!(lenient(&wire).components(), &[invoke_58(1)]);

    let found = problem(&wire);
    assert_eq!(
        found.fault,
        Fault::Component {
            index: 1,
            component_type: Some(ComponentType::ReturnResultLast),
            invoke_id: Some(2),
            problem: GeneralProblem::MISTYPED_COMPONENT,
        }
    );
    assert_eq!(
        found.partial.as_ref().map(TcapMessage::components),
        Some(&[invoke_58(1)][..])
    );
}

#[test]
fn defect_2_the_cases_rasn_already_refused_now_carry_the_same_information() {
    // A malformed component before a good one: rasn stops, finds octets left
    // over and fails. The crate reports it as a fault of component 0, and per
    // 3.2.2.2/Q.774 the good component after it is discarded.
    let wire = vector(
        "62 15  48 04 00 00 10 01
            6c 0d
               a1 03 02 01 02            -- invoke 2 without an operation code
               a1 06 02 01 01 02 01 3a   -- invoke 1, operation 58",
    );
    assert!(rasn::ber::decode::<TcapMessage>(&wire).is_err());
    let found = problem(&wire);
    assert!(matches!(
        found.fault,
        Fault::Component {
            index: 0,
            invoke_id: Some(2),
            ..
        }
    ));
    assert_eq!(
        found.partial.as_ref().map(|m| m.components().len()),
        Some(0)
    );

    // An unknown component tag.
    let wire = vector(
        "62 10  48 04 00 00 10 01
            6c 08
               a5 06 02 01 01 02 01 3a   -- [5] is not a component type",
    );
    assert!(rasn::ber::decode::<TcapMessage>(&wire).is_err());
    let found = problem(&wire);
    assert_eq!(
        found.fault,
        Fault::Component {
            index: 0,
            component_type: None,
            invoke_id: None,
            problem: GeneralProblem::UNRECOGNIZED_COMPONENT,
        }
    );
    assert_eq!(
        found.reject(),
        Some(Reject::general(
            None,
            GeneralProblem::UNRECOGNIZED_COMPONENT
        ))
    );
}

// ── 3. Octets after the outermost message ───────────────────────────────────

#[test]
fn defect_3_octets_after_the_message_are_an_error() {
    // A complete Begin, and after its end a whole further component.
    let wire = vector(
        "62 10                      -- Begin, 16 octets, complete
            48 04 00 00 10 01
            6c 08
               a1 06 02 01 01 02 01 3a
         a1 06 02 01 02 02 01 3a    -- 8 octets past the end of the message",
    );

    assert_eq!(
        lenient(&wire).components(),
        &[invoke_58(1)],
        "rasn ignores what follows the message"
    );

    let found = problem(&wire);
    assert_eq!(found.sublayer(), Sublayer::Transaction);
    assert_eq!(
        found.p_abort_cause(),
        Some(PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION)
    );
    assert!(found.detail.contains("8 octets follow"), "{}", found.detail);
    // The originating transaction ID is derivable, so the message is answered
    // with an Abort addressed to it.
    assert_eq!(
        found.abort(),
        Some(Abort::p_abort(
            vec![0x00, 0x00, 0x10, 0x01].into(),
            PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION
        ))
    );
    assert_eq!(found.partial, None, "the whole message is discarded");

    // Wireshark, for the record, dissects the message and passes over the
    // extra octets without a marker. SCCP hands over the user data with its
    // length, so this crate does not.
    if let Some(d) = dissect_unchecked(&wire) {
        d.hex("tcap.otid", "00001001");
        assert_eq!(d.problems(), Vec::<String>::new());
    }
}

// ── 4. A malformed dialogue PDU ─────────────────────────────────────────────

/// A Begin whose AARQ holds an INTEGER where the application context name
/// (an OBJECT IDENTIFIER) belongs.
const MALFORMED_AARQ: &str = "
    62 20                      -- Begin, 6 + 26 = 32
       48 04 00 00 10 01
       6b 18                   -- DialoguePortion
          28 16                -- EXTERNAL, 9 + 13 = 22
             06 07 00 11 86 05 01 01 01   -- dialogue-as
             a0 0b
                60 09          -- AARQ-apdu
                   80 02 07 80            -- protocol-version
                   a1 03 02 01 05         -- application-context-name [1] { INTEGER 5 }";

/// A Begin whose dialogue portion is a well-formed EXTERNAL in a user-defined
/// abstract syntax (2.999.1, under the example arc), holding an OCTET STRING.
const USER_DEFINED_SYNTAX: &str = "
    62 14                      -- Begin, 6 + 14 = 20
       48 04 00 00 10 01
       6b 0c                   -- DialoguePortion
          28 0a                -- EXTERNAL, 5 + 5 = 10
             06 03 88 37 01    -- direct-reference 2.999.1
             a0 03 04 01 01    -- single-ASN1-type { OCTET STRING 01 }";

#[test]
fn defect_4_a_malformed_dialogue_pdu_is_reported_as_malformed() {
    let wire = vector(MALFORMED_AARQ);

    // rasn has nothing to say about the dialogue portion: the member is an
    // open type and comes back as opaque octets.
    let opaque = lenient(&wire);
    let portion = opaque.dialogue_portion().expect("dialogue portion present");
    // 1.0.0 answered "no dialogue PDU" here. Now it is an error.
    let error = portion.parse().unwrap_err();
    assert!(
        error.detail().contains("application-context-name"),
        "{error}"
    );
    assert!(portion.dialogue_pdu().is_err());

    // The decoder does not hand out the message at all.
    let found = problem(&wire);
    assert_eq!(found.fault, Fault::DialoguePortion);
    assert_eq!(found.sublayer(), Sublayer::Component);
    assert_eq!(found.partial, None);
    // 3.2.2.1/Q.774: an ABRT APDU, abort-source dialogue-service-provider, no
    // user information, sent in an Abort to the originating transaction ID.
    let abort = found.abort().expect("an Abort answers a Begin");
    assert_eq!(
        abort,
        Abort::u_abort(
            vec![0x00, 0x00, 0x10, 0x01].into(),
            Some(DialoguePortion::abnormal_dialogue())
        )
    );
    assert_eq!(found.reject(), None);
}

#[test]
fn defect_4_a_pdu_the_crate_does_not_model_is_not_an_error() {
    let message = tcap::decode(&vector(USER_DEFINED_SYNTAX)).expect("well-formed message");
    let portion = message
        .dialogue_portion()
        .expect("dialogue portion present");
    match portion.parse().expect("well-formed dialogue portion") {
        DialogueContent::Unmodelled(external) => {
            assert_eq!(external.direct_reference.unwrap().to_string(), "2.999.1");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(portion.dialogue_pdu(), Ok(None));
}

#[test]
fn defect_4_an_absent_dialogue_portion_is_a_third_thing() {
    let message = tcap::decode(&vector("62 06 48 04 00 00 10 01")).unwrap();
    assert!(message.dialogue_portion().is_none());
}
