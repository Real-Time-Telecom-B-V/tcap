//! What the decoder reports for a damaged message, against ITU-T Q.772 and
//! Q.774 (06/97).
//!
//! Q.772 gives the meaning of each P-Abort cause (2.3) and of each general
//! problem (3.7.1), with example mappings in its Tables 1 and 2. Q.774 says
//! who acts on what: the transaction sub-layer on the transaction portion
//! (3.3.4, Table 7), the component sub-layer on the dialogue portion
//! (3.2.2.1) and on each component (3.2.2.2, Table 5). The tests follow those
//! tables case by case.
//!
//! All values are synthetic.

mod common;

use common::{known_answer, problem, vector};
use tcap::{
    Abort, AbortSource, Begin, Component, ComponentType, Decoded, DialoguePortion, Fault,
    GeneralProblem, Invoke, MessageType, OperationCode, PAbortCause, Reject, ReturnResult,
    Sublayer, TcapError, TcapMessage,
};

const OTID: [u8; 4] = [0x00, 0x00, 0x10, 0x01];
const DTID: [u8; 4] = [0x00, 0x00, 0x20, 0x02];

/// A transaction portion fault with this cause; returns the problem.
#[track_caller]
fn transaction_fault(wire: &str, cause: PAbortCause) -> tcap::DecodeProblem {
    let found = problem(&vector(wire));
    assert_eq!(found.sublayer(), Sublayer::Transaction, "{found}");
    assert_eq!(found.p_abort_cause(), Some(cause), "{found}");
    assert_eq!(found.partial, None, "the erroneous message is discarded");
    assert_eq!(found.reject(), None);
    found
}

/// The Abort that answers a problem addressed to [`OTID`] with this cause.
fn p_abort(cause: PAbortCause) -> Option<Abort> {
    Some(Abort::p_abort(OTID.to_vec().into(), cause))
}

// ── Transaction sub-layer: unrecognized message type (2.3.1/Q.772) ──────────

#[test]
fn an_unknown_message_type_with_a_derivable_otid_is_aborted() {
    // Table 8/Q.773 marks [APPLICATION 3] and [APPLICATION 6] reserved.
    // Table 7/Q.774, UNKNOWN, originating ID derivable: Abort.
    for tag in ["63", "66", "68", "7f 20"] {
        let found = transaction_fault(
            &format!("{tag} 06  48 04 00 00 10 01"),
            PAbortCause::UNRECOGNIZED_MESSAGE_TYPE,
        );
        assert_eq!(found.message_type, None);
        assert_eq!(found.otid.as_deref(), Some(&OTID[..]));
        assert_eq!(
            found.abort(),
            p_abort(PAbortCause::UNRECOGNIZED_MESSAGE_TYPE)
        );
    }
}

#[test]
fn the_abort_for_an_unknown_message_type_on_the_wire() {
    let found = problem(&vector("63 06  48 04 00 00 10 01"));
    known_answer(
        &TcapMessage::Abort(found.abort().unwrap()),
        "67 09                   -- Abort
            49 04 00 00 10 01    -- dtid = the otid of the message being answered
            4a 01 00             -- P-AbortCause unrecognizedMessageType (0)",
    );
}

#[test]
fn an_unknown_message_type_without_an_otid_is_discarded() {
    // Table 7/Q.774, UNKNOWN, originating ID not derivable: Discard.
    // "The combination of class, form and value does not correspond to a
    // known tag" (Table 1/Q.772): a universal SEQUENCE, a context tag, and
    // the Begin tag number in primitive form.
    for wire in [
        "30 03 02 01 00",
        "a2 03 02 01 00",
        "42 06 48 04 00 00 10 01",
    ] {
        let found = transaction_fault(wire, PAbortCause::UNRECOGNIZED_MESSAGE_TYPE);
        assert_eq!(found.otid, None, "{wire}");
        assert_eq!(found.abort(), None, "{wire}");
    }
}

// ── Transaction sub-layer: badly formatted (2.3.3/Q.772) ────────────────────

#[test]
fn a_length_that_does_not_match_the_message_is_badly_formatted() {
    // "Length indicator value does not correspond to length of message."
    // Too long: the Begin announces 25 octets and 14 follow. The otid sits in
    // front of the damage and is derivable.
    let found = transaction_fault(
        "62 19  48 04 00 00 10 01  6c 11 a1 0f 02 01 01 02",
        PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION,
    );
    assert_eq!(found.message_type, Some(MessageType::Begin));
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION)
    );

    // Too short: octets follow the end of the message.
    let found = transaction_fault(
        "62 06  48 04 00 00 10 01  00",
        PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION,
    );
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION)
    );
}

#[test]
fn an_element_that_overruns_the_message_is_badly_formatted() {
    // The component portion announces 5 octets at the very end of the message.
    let found = transaction_fault(
        "62 08  48 04 00 00 10 01  6c 05",
        PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION,
    );
    assert_eq!(found.otid.as_deref(), Some(&OTID[..]));
}

#[test]
fn the_wrong_form_for_an_element_is_badly_formatted() {
    // "Malformed tag for an information element, other than Message Type
    // (e.g. the class and code indicates integer while the form indicates
    // constructed encoding)."
    for wire in [
        // otid constructed (an OCTET STRING is primitive, 4.1.1/Q.773)
        "62 08  68 06 04 04 00 00 10 01",
        // dialogue portion primitive
        "62 08  48 04 00 00 10 01  4b 00",
        // component portion primitive
        "62 0a  48 04 00 00 10 01  4c 02 05 00",
        // P-Abort cause constructed
        "67 0b  49 04 00 00 20 02  6a 03 02 01 00",
    ] {
        transaction_fault(wire, PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION);
    }
}

#[test]
fn nothing_at_all_is_badly_formatted_and_discarded() {
    for wire in ["", "62", "62 82"] {
        let found = transaction_fault(wire, PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION);
        assert_eq!(found.abort(), None, "nothing to address an Abort to");
    }
}

// ── Transaction sub-layer: incorrect transaction portion (2.3.4/Q.772) ──────

#[test]
fn transaction_ids_that_do_not_fit_the_message_type_are_incorrect() {
    // "Combination of Origin and Destination Transactions ID does not conform
    // to message type." (Table 9/Q.773)

    // Begin with a dtid instead of an otid: nothing to answer to.
    let found = transaction_fault(
        "62 06  49 04 00 00 20 02",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!((found.otid.clone(), found.abort()), (None, None));

    // Continue without a dtid. Table 7/Q.774, CONTINUE, originating ID
    // derivable, destination ID not derivable: Abort.
    let found = transaction_fault(
        "65 06  48 04 00 00 10 01",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!(found.dtid, None);
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::INCORRECT_TRANSACTION_PORTION)
    );

    // End with an otid: an End is not answered (Table 7/Q.774: Discard).
    let found = transaction_fault(
        "64 06  48 04 00 00 10 01",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!(found.message_type, Some(MessageType::End));
    assert_eq!(found.abort(), None);
}

#[test]
fn elements_out_of_order_are_incorrect() {
    // "The order of the received information elements within the message does
    // not conform to Recommendation Q.773 for the message type."
    // Continue with the dtid before the otid. Both IDs are derivable.
    let found = transaction_fault(
        "65 0c  49 04 00 00 20 02  48 04 00 00 10 01",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!(found.otid.as_deref(), Some(&OTID[..]));
    assert_eq!(found.dtid.as_deref(), Some(&DTID[..]));
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::INCORRECT_TRANSACTION_PORTION)
    );

    // Begin with the component portion before the dialogue portion.
    transaction_fault(
        "62 1e  48 04 00 00 10 01
                6c 08 a1 06 02 01 01 02 01 3a
                6b 0c 28 0a 06 03 88 37 01 a0 03 04 01 01",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
}

#[test]
fn a_missing_mandatory_element_is_incorrect() {
    // "Message does not contain all the mandatory information elements".
    // A Unidirectional has to have a component portion. It carries no
    // transaction ID, so it is discarded (Table 7/Q.774).
    let found = transaction_fault("61 00", PAbortCause::INCORRECT_TRANSACTION_PORTION);
    assert_eq!(found.message_type, Some(MessageType::Unidirectional));
    assert_eq!(found.abort(), None);

    // An Abort without a dtid.
    transaction_fault(
        "67 03  4a 01 00",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
}

#[test]
fn a_component_portion_without_components_is_incorrect() {
    // "Component Portion Tag present, but no components."
    let found = transaction_fault(
        "62 08  48 04 00 00 10 01  6c 00",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::INCORRECT_TRANSACTION_PORTION)
    );
}

#[test]
fn an_element_the_message_type_does_not_have_is_incorrect() {
    for wire in [
        // an OCTET STRING after the otid
        "62 09  48 04 00 00 10 01  04 01 00",
        // a second otid
        "62 0c  48 04 00 00 10 01  48 04 00 00 10 01",
        // a P-Abort cause in a Begin
        "62 09  48 04 00 00 10 01  4a 01 00",
        // a component portion in an Abort
        "67 0d  49 04 00 00 20 02  6c 05 a2 03 02 01 01",
        // an Abort with a P-Abort cause and user abort information: the
        // reason is a CHOICE of the two
        "67 17  49 04 00 00 20 02  4a 01 00
                6b 0c 28 0a 06 03 88 37 01 a0 03 04 01 01",
    ] {
        transaction_fault(wire, PAbortCause::INCORRECT_TRANSACTION_PORTION);
    }
}

#[test]
fn a_transaction_id_of_the_wrong_length_is_incorrect_and_not_derivable() {
    // OrigTransactionID ::= [APPLICATION 8] IMPLICIT OCTET STRING (SIZE (1..4)).
    // An ID that is too long cannot be the dtid of an Abort either, so the
    // message is discarded.
    for wire in ["62 02  48 00", "62 07  48 05 01 02 03 04 05"] {
        let found = transaction_fault(wire, PAbortCause::INCORRECT_TRANSACTION_PORTION);
        assert_eq!(found.otid, None, "{wire}");
        assert_eq!(found.abort(), None, "{wire}");
    }

    // Continue with a good otid and a dtid of five octets: answered.
    let found = transaction_fault(
        "65 0d  48 04 00 00 10 01  49 05 01 02 03 04 05",
        PAbortCause::INCORRECT_TRANSACTION_PORTION,
    );
    assert_eq!(found.dtid, None);
    assert_eq!(
        found.abort(),
        p_abort(PAbortCause::INCORRECT_TRANSACTION_PORTION)
    );
}

#[test]
fn a_damaged_end_or_abort_is_never_answered() {
    // Table 7/Q.774, END/ABORT: Discard. The destination transaction ID is
    // there for the caller to release its own transaction if it is assigned.
    for wire in [
        "64 08  49 04 00 00 20 02  6c 00",
        "67 08  49 04 00 00 20 02  4a 00",
    ] {
        let found = problem(&vector(wire));
        assert_eq!(found.sublayer(), Sublayer::Transaction);
        assert_eq!(found.dtid.as_deref(), Some(&DTID[..]));
        assert_eq!(found.abort(), None);
    }
}

#[test]
fn an_abort_with_an_unnamed_cause_is_a_valid_abort() {
    // P-AbortCause is an INTEGER with named numbers; a value without a name
    // is still a value. The transaction is aborted all the same.
    let message = tcap::decode(&vector("67 09  49 04 00 00 20 02  4a 01 7f")).unwrap();
    match message {
        TcapMessage::Abort(abort) => {
            assert_eq!(abort.p_abort_cause(), Some(PAbortCause(127)));
            assert_eq!(PAbortCause(127).name(), None);
        }
        other => panic!("{other:?}"),
    }
}

// ── Component sub-layer: the dialogue portion (3.2.2.1/Q.774) ───────────────

/// The Abort that answers an incorrect dialogue portion.
fn abnormal_dialogue_abort() -> Option<Abort> {
    Some(Abort::u_abort(
        OTID.to_vec().into(),
        Some(DialoguePortion::abnormal_dialogue()),
    ))
}

#[track_caller]
fn dialogue_fault(wire: &str) -> tcap::DecodeProblem {
    let found = problem(&vector(wire));
    assert_eq!(found.fault, Fault::DialoguePortion, "{found}");
    assert_eq!(found.sublayer(), Sublayer::Component);
    assert_eq!(found.p_abort_cause(), None);
    assert_eq!(found.reject(), None);
    assert_eq!(found.partial, None, "the components are discarded with it");
    found
}

#[test]
fn an_incorrect_dialogue_portion_in_a_begin_is_answered_with_an_abrt() {
    // The dialogue portion holds a SEQUENCE where the EXTERNAL belongs. The
    // Invoke after it is sound, and is discarded: "If any components are
    // received in the message with an incorrect dialogue portion, these are
    // discarded."
    let found = dialogue_fault(
        "62 17  48 04 00 00 10 01
                6b 05 30 03 02 01 00
                6c 08 a1 06 02 01 01 02 01 3a",
    );
    assert_eq!(found.abort(), abnormal_dialogue_abort());
}

#[test]
fn the_abort_for_an_incorrect_dialogue_portion_on_the_wire() {
    // "a TR-U-ABORT request primitive is issued to the transaction sub-layer
    // with an ABRT APDU as user data. The abort-source field of the ABRT APDU
    // is set to "dialogue-service-provider" and the user information field is
    // absent."
    let abort = abnormal_dialogue_abort().unwrap();
    let bytes = known_answer(
        &TcapMessage::Abort(abort),
        "67 1a                      -- Abort
            49 04 00 00 10 01       -- dtid = the otid of the message being answered
            6b 12                   -- u-abortCause
               28 10
                  06 07 00 11 86 05 01 01 01   -- dialogue-as
                  a0 05
                     64 03          -- ABRT-apdu
                        80 01 01    -- abort-source dialogue-service-provider (1)",
    );
    if let Some(d) = common::dissect(&bytes) {
        d.hex("tcap.dtid", "00001001")
            .present("tcap.dialogueAbort_element")
            .show("tcap.abort_source", "1")
            .absent("tcap.abrt_user_information");
    }
    assert_eq!(
        DialoguePortion::abnormal_dialogue(),
        DialoguePortion::abrt(AbortSource::DialogueServiceProvider)
    );
}

#[test]
fn an_incorrect_dialogue_portion_in_a_continue_is_answered_too() {
    // An AARE whose result is 5, which Q.773 does not define.
    let found = dialogue_fault(
        "65 38  48 04 00 00 10 01  49 04 00 00 20 02
                6b 2a 28 28 06 07 00 11 86 05 01 01 01
                   a0 1d 61 1b 80 02 07 80
                      a1 09 06 07 04 00 00 01 00 1a 02
                      a2 03 02 01 05
                      a3 05 a1 03 02 01 00",
    );
    assert_eq!(found.dtid.as_deref(), Some(&DTID[..]));
    assert_eq!(found.abort(), abnormal_dialogue_abort());
}

#[test]
fn an_incorrect_dialogue_portion_in_an_end_an_abort_or_a_unidirectional_is_local() {
    // No transaction is left to answer in (End, Abort), or there never was
    // one (Unidirectional). The local TC-user is told; nothing is sent.
    for wire in [
        "64 0d  49 04 00 00 20 02  6b 05 30 03 02 01 00",
        "67 0d  49 04 00 00 20 02  6b 05 30 03 02 01 00",
        "61 0e  6b 05 30 03 02 01 00  6c 05 a2 03 02 01 01",
    ] {
        assert_eq!(dialogue_fault(wire).abort(), None, "{wire}");
    }
}

#[test]
fn an_empty_dialogue_portion_is_incorrect() {
    dialogue_fault("62 08  48 04 00 00 10 01  6b 00");
}

// ── Component sub-layer: components (3.2.2.2/Q.774, Table 5) ────────────────

#[track_caller]
fn component_fault(
    portion: &str,
    index: usize,
    component_type: Option<ComponentType>,
    invoke_id: Option<i8>,
    general: GeneralProblem,
) -> tcap::DecodeProblem {
    // A Continue around the component portion content given.
    let content = vector(portion);
    let mut wire = vec![0x65, (14 + content.len()) as u8];
    wire.extend_from_slice(&[0x48, 0x04]);
    wire.extend_from_slice(&OTID);
    wire.extend_from_slice(&[0x49, 0x04]);
    wire.extend_from_slice(&DTID);
    wire.extend_from_slice(&[0x6c, content.len() as u8]);
    wire.extend_from_slice(&content);

    let found = problem(&wire);
    assert_eq!(
        found.fault,
        Fault::Component {
            index,
            component_type,
            invoke_id,
            problem: general,
        },
        "{found}"
    );
    assert_eq!(found.sublayer(), Sublayer::Component);
    assert_eq!(found.abort(), None, "a component is rejected, not aborted");
    assert_eq!(found.p_abort_cause(), None);
    assert_eq!(found.otid.as_deref(), Some(&OTID[..]));
    assert_eq!(found.dtid.as_deref(), Some(&DTID[..]));
    let partial = found.partial.as_ref().expect("the message so far");
    assert_eq!(partial.components().len(), index);
    assert_eq!(partial.message_type(), MessageType::Continue);
    found
}

#[test]
fn an_unrecognized_component_type_is_rejected_without_an_invoke_id() {
    // 3.7.1.1/Q.772: "The component type is not recognized as being one of
    // those defined". Table 5/Q.774, Unknown: Initiate Reject.
    for portion in [
        "a5 06 02 01 01 02 01 3a", // [5]
        "a6 03 02 01 01",          // [6]
        "30 03 02 01 01",          // a universal SEQUENCE
        "81 01 01",                // [1] in primitive form
    ] {
        let found = component_fault(
            portion,
            0,
            None,
            None,
            GeneralProblem::UNRECOGNIZED_COMPONENT,
        );
        assert_eq!(
            found.reject(),
            Some(Reject::general(
                None,
                GeneralProblem::UNRECOGNIZED_COMPONENT
            ))
        );
    }
}

#[test]
fn a_mistyped_component_is_rejected_with_its_invoke_id() {
    // 3.7.1.2/Q.772 and Table 2/Q.772: "Operation code element expected but
    // not present", "Return Error Component received with no Error Code
    // Element".
    let cases = [
        ("a1 03 02 01 07", ComponentType::Invoke),
        ("a3 03 02 01 07", ComponentType::ReturnError),
        // an Invoke with two parameters
        (
            "a1 0c 02 01 07 02 01 3a 04 01 01 04 01 02",
            ComponentType::Invoke,
        ),
        // a linked ID of 300, outside INTEGER (-128..127)
        ("a1 0a 02 01 07 80 02 01 2c 02 01 3a", ComponentType::Invoke),
        // a ReturnResult whose result is not a SEQUENCE
        ("a2 06 02 01 07 02 01 3a", ComponentType::ReturnResultLast),
        // a ReturnResult whose result holds something after the parameter
        (
            "a7 0d 02 01 07 30 08 02 01 3a 05 00 04 01 01",
            ComponentType::ReturnResultNotLast,
        ),
    ];
    for (portion, component_type) in cases {
        let found = component_fault(
            portion,
            0,
            Some(component_type),
            Some(7),
            GeneralProblem::MISTYPED_COMPONENT,
        );
        assert_eq!(
            found.reject(),
            Some(Reject::general(Some(7), GeneralProblem::MISTYPED_COMPONENT)),
            "{portion}"
        );
    }
}

#[test]
fn a_mistyped_component_without_a_readable_invoke_id_is_rejected_with_null() {
    // Table 2/Q.772: "Missing Invoke ID element."
    for portion in [
        // the operation code comes first
        "a1 03 06 01 2a",
        // an empty Invoke
        "a1 00",
        // an invoke ID of two octets is outside INTEGER (-128..127)
        "a1 07 02 02 01 2c 02 01 3a",
    ] {
        let found = component_fault(
            portion,
            0,
            Some(ComponentType::Invoke),
            None,
            GeneralProblem::MISTYPED_COMPONENT,
        );
        assert_eq!(
            found.reject(),
            Some(Reject::general(None, GeneralProblem::MISTYPED_COMPONENT))
        );
    }
}

#[test]
fn the_reject_for_a_mistyped_component_on_the_wire() {
    let found = component_fault(
        "a1 03 02 01 07",
        0,
        Some(ComponentType::Invoke),
        Some(7),
        GeneralProblem::MISTYPED_COMPONENT,
    );
    // The Reject goes out with the TC-user's next Continue or End.
    let message = TcapMessage::End(tcap::End {
        dtid: found.otid.clone().unwrap(),
        dialogue_portion: None,
        components: Some(vec![Component::Reject(found.reject().unwrap())]),
    });
    let bytes = known_answer(
        &message,
        "64 10  49 04 00 00 10 01
            6c 08
               a4 06                -- reject
                  02 01 07          -- invokeID derivable: the one that was read
                  80 01 01          -- generalProblem mistypedComponent (1)",
    );
    if let Some(d) = common::dissect(&bytes) {
        d.show("gsm_old.derivable", "7")
            .show("gsm_old.generalProblem", "1");
    }
}

#[test]
fn a_badly_structured_component_is_rejected() {
    // 3.7.1.3/Q.772: "The contents of the component do not conform to the
    // encoding rules".

    // The operation code announces five octets and one is there. The invoke
    // ID in front of it can be read.
    component_fault(
        "a1 06 02 01 07 02 05 3a",
        0,
        Some(ComponentType::Invoke),
        Some(7),
        GeneralProblem::BADLY_STRUCTURED_COMPONENT,
    );

    // The same inside the result SEQUENCE of a ReturnResult.
    component_fault(
        "a2 08 02 01 07 30 03 02 05 3a",
        0,
        Some(ComponentType::ReturnResultLast),
        Some(7),
        GeneralProblem::BADLY_STRUCTURED_COMPONENT,
    );

    // The component itself announces more than the component portion holds.
    let found = component_fault(
        "a1 0f 02 01 07 02 01 3a",
        0,
        Some(ComponentType::Invoke),
        Some(7),
        GeneralProblem::BADLY_STRUCTURED_COMPONENT,
    );
    assert_eq!(
        found.reject(),
        Some(Reject::general(
            Some(7),
            GeneralProblem::BADLY_STRUCTURED_COMPONENT
        ))
    );

    // An element in the wrong form. rasn would read the content octets of the
    // constructed [0] as the linked ID (5); the sender wrote something else.
    component_fault(
        "a1 09 02 01 07 a0 01 05 02 01 3a",
        0,
        Some(ComponentType::Invoke),
        Some(7),
        GeneralProblem::BADLY_STRUCTURED_COMPONENT,
    );

    // One stray octet where a component should start.
    component_fault(
        "a2 03 02 01 01  a1",
        1,
        None,
        None,
        GeneralProblem::BADLY_STRUCTURED_COMPONENT,
    );
}

#[test]
fn an_argument_is_the_tc_users_to_judge() {
    // The parameter is one well-delimited element whose inside is broken.
    // That is a mistyped parameter for the TC-user (an invoke problem), not a
    // component the component sub-layer cannot read.
    let wire = vector(
        "62 15  48 04 00 00 10 01
            6c 0d
               a1 0b 02 01 01 02 01 3a
                  30 03 02 05 3a          -- SEQUENCE { INTEGER of 5 octets, 1 present }",
    );
    let message = tcap::decode(&wire).unwrap();
    match &message.components()[0] {
        Component::Invoke(invoke) => {
            assert_eq!(
                invoke.parameter.as_ref().unwrap().as_bytes(),
                &[0x30, 0x03, 0x02, 0x05, 0x3a]
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn components_before_the_faulty_one_stand_and_those_after_are_discarded() {
    // 3.2.2.2/Q.774: "In the case of multiple components within a message,
    // when a malformed component is detected by the component sub-layer,
    // subsequent components in the message are discarded."
    let found = component_fault(
        "a1 06 02 01 01 02 01 3a    -- invoke 1
         a2 03 02 01 02             -- returnResultLast 2
         a1 03 02 01 03             -- invoke 3 without an operation code
         a1 06 02 01 04 02 01 3a    -- invoke 4, sound, and discarded",
        2,
        Some(ComponentType::Invoke),
        Some(3),
        GeneralProblem::MISTYPED_COMPONENT,
    );
    assert_eq!(
        found.partial.unwrap().components(),
        &[
            Component::Invoke(Invoke {
                invoke_id: 1,
                linked_id: None,
                operation_code: OperationCode::Local(58),
                parameter: None,
            }),
            Component::ReturnResultLast(ReturnResult {
                invoke_id: 2,
                result: None,
            }),
        ]
    );
}

#[test]
fn a_malformed_reject_is_not_answered_with_a_reject() {
    // 3.2.2.2/Q.774: "If the incorrect component is itself a Reject
    // component, the component is discarded and the local TC-user is advised
    // of the syntax error in the received Reject component."
    for portion in [
        "a4 03 02 01 07",          // no problem code
        "a4 06 02 01 07 85 01 00", // [5] is not a problem class
        "a4 03 80 01 00",          // no invoke ID and no NULL
    ] {
        let wire = {
            let content = vector(portion);
            let mut wire = vec![0x65, (14 + content.len()) as u8, 0x48, 0x04];
            wire.extend_from_slice(&OTID);
            wire.extend_from_slice(&[0x49, 0x04]);
            wire.extend_from_slice(&DTID);
            wire.extend_from_slice(&[0x6c, content.len() as u8]);
            wire.extend_from_slice(&content);
            wire
        };
        let found = problem(&wire);
        assert!(
            matches!(
                found.fault,
                Fault::Component {
                    component_type: Some(ComponentType::Reject),
                    problem: GeneralProblem::MISTYPED_COMPONENT,
                    ..
                }
            ),
            "{found}"
        );
        assert_eq!(found.reject(), None, "{portion}");
        assert_eq!(found.abort(), None);
    }
}

#[test]
fn a_reject_whose_problem_is_in_the_constructed_form_is_not_read_as_a_number() {
    // generalProblem [0] IMPLICIT INTEGER is 80 01 01. A0 03 02 01 01 wraps
    // the INTEGER in a constructed tag; rasn takes the three content octets
    // for the value and returns general problem 131329.
    let wire = vector("64 12  49 04 00 00 20 02  6c 0a  a4 08 02 01 07 a0 03 02 01 01");
    let lenient: TcapMessage = rasn::ber::decode(&wire).unwrap();
    assert_eq!(
        lenient.components(),
        &[Component::Reject(Reject::general(
            Some(7),
            GeneralProblem(0x02_0101)
        ))]
    );

    let found = problem(&wire);
    assert_eq!(
        found.fault,
        Fault::Component {
            index: 0,
            component_type: Some(ComponentType::Reject),
            invoke_id: Some(7),
            problem: GeneralProblem::BADLY_STRUCTURED_COMPONENT,
        }
    );
    assert_eq!(found.reject(), None, "a Reject is not rejected");
}

#[test]
fn a_reject_with_an_unnamed_problem_code_is_a_valid_reject() {
    let wire = vector("64 10  49 04 00 00 20 02  6c 08  a4 06 02 01 07 81 01 63");
    let message = tcap::decode(&wire).unwrap();
    assert_eq!(
        message.components(),
        &[Component::Reject(Reject::invoke(
            7,
            tcap::InvokeProblem(99)
        ))]
    );
}

#[test]
fn a_result_with_the_operation_code_alone_is_read() {
    // Q.773 has the parameter of the result sequence mandatory, and the
    // encoder holds to that. Receivers in the field accept the operation code
    // alone (Wireshark's grammar has the parameter OPTIONAL), and nothing is
    // lost by reading it.
    let wire = vector("64 12  49 04 00 00 20 02  6c 0a  a2 08 02 01 01 30 03 02 01 3a");
    let message = tcap::decode(&wire).unwrap();
    match &message.components()[0] {
        Component::ReturnResultLast(result) => {
            let value = result.result.as_ref().unwrap();
            assert_eq!(value.operation_code, OperationCode::Local(58));
            assert_eq!(value.parameter, None);
        }
        other => panic!("{other:?}"),
    }
    if let Some(d) = common::dissect(&wire) {
        d.present("gsm_old.resultretres_element")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn an_integer_in_more_octets_than_needed_is_read() {
    // X.690 8.3.2 has an INTEGER in the fewest octets. An encoder that pads
    // one (here the invoke ID 7 as 00 07) has sent the same value, and
    // nothing is lost by reading it.
    let wire = vector("64 11  49 04 00 00 20 02  6c 09  a1 07 02 02 00 07 02 01 3a");
    let message = tcap::decode(&wire).unwrap();
    assert_eq!(message.components()[0].invoke_id(), Some(7));
}

// ── The two entry points agree ──────────────────────────────────────────────

#[test]
fn decode_fails_with_the_problem_attached() {
    let wire = vector("62 0d  48 04 00 00 10 01  6c 05 a1 03 02 01 02");
    let error = tcap::decode(&wire).unwrap_err();
    let attached = error.problem().expect("the problem travels with the error");
    assert_eq!(
        attached.reject(),
        Some(Reject::general(Some(2), GeneralProblem::MISTYPED_COMPONENT))
    );
    let text = error.to_string();
    assert!(text.contains("component 0"), "{text}");
    assert!(text.contains("mistypedComponent"), "{text}");
    assert!(matches!(error, TcapError::Malformed(_)));
}

#[test]
fn a_sound_message_is_complete() {
    let message = TcapMessage::Begin(Begin {
        otid: OTID.to_vec().into(),
        dialogue_portion: None,
        components: None,
    });
    let wire = tcap::encode(&message).unwrap();
    assert_eq!(
        tcap::decode_detailed(&wire),
        Decoded::Complete(message.clone())
    );
    assert_eq!(tcap::decode_detailed(&wire).problem(), None);
    assert_eq!(
        tcap::decode_detailed(&wire).into_result().ok(),
        Some(message)
    );
}

#[test]
fn length_forms_the_encoder_does_not_use_are_read() {
    // 4.1.1/Q.773 has a sender use the short form below 128 octets. A long
    // form there, or the indefinite form, loses nothing and is read.
    let short = vector("62 10  48 04 00 00 10 01  6c 08  a1 06 02 01 01 02 01 3a");
    let long = vector("62 81 12  48 04 00 00 10 01  6c 81 09  a1 81 06 02 01 01 02 01 3a");
    let indefinite =
        vector("62 80  48 04 00 00 10 01  6c 80  a1 80 02 01 01 02 01 3a 00 00  00 00  00 00");
    let want = tcap::decode(&short).unwrap();
    assert_eq!(tcap::decode(&long).unwrap(), want);
    assert_eq!(tcap::decode(&indefinite).unwrap(), want);
}

#[test]
fn a_parameter_in_the_indefinite_form_is_carried_as_received() {
    // The argument is the TC-user's, octet for octet, end-of-contents included.
    let wire = vector(
        "62 17  48 04 00 00 10 01
            6c 0f
               a1 0d 02 01 01 02 01 3a
                  30 80 04 01 01 00 00    -- SEQUENCE, indefinite length",
    );
    let message = tcap::decode(&wire).unwrap();
    match &message.components()[0] {
        Component::Invoke(invoke) => assert_eq!(
            invoke.parameter.as_ref().unwrap().as_bytes(),
            &[0x30, 0x80, 0x04, 0x01, 0x01, 0x00, 0x00]
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(tcap::encode(&message).unwrap(), wire);
}

#[test]
fn deep_nesting_does_not_exhaust_the_stack() {
    // 5000 nested indefinite-length elements where the message should be.
    let mut wire = Vec::new();
    for _ in 0..5000 {
        wire.extend_from_slice(&[0x62, 0x80]);
    }
    for _ in 0..5000 {
        wire.extend_from_slice(&[0x00, 0x00]);
    }
    let found = problem(&wire);
    assert_eq!(
        found.p_abort_cause(),
        Some(PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION)
    );
}
