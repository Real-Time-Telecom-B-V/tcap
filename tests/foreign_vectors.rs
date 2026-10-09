//! Decoding bytes this crate did not produce.
//!
//! The vectors below were encoded by pyasn1 from a transcription of the ASN.1
//! of ITU-T Q.773 clauses 3.1 and 3.2 (`scripts/foreign_vectors.py`; rerun it
//! to regenerate them). pyasn1 shares no code with this crate or with rasn,
//! so a mistake in this crate's reading of a tag or of a wrapper shows up as
//! a failed decode or as a different value.
//!
//! Each message comes in two forms: with definite lengths, and with the
//! indefinite form on every constructed element (4.1.2.3/Q.773: "The
//! indefinite form ... may (but need not) be used ... whenever the element is
//! a constructor"), which this crate accepts and never emits.
//!
//! All values are synthetic.

mod common;

use common::vector;
use rasn::types::{Any, ObjectIdentifier, Oid};
use tcap::{
    Abort, AbortSource, AssociateResult, AssociateSourceDiagnostic, Begin, Component, Continue,
    DialogueContent, DialoguePdu, DialoguePortion, End, ErrorCode, External, GeneralProblem,
    Invoke, InvokeProblem, OperationCode, PAbortCause, ProtocolVersion, Reject, ReturnError,
    ReturnErrorProblem, ReturnResult, ReturnResultProblem, ReturnResultValue, TcapMessage,
    Unidirectional,
};

/// name, definite-length form, indefinite-length form.
const VECTORS: &[(&str, &str, &str)] = &[
    (
        "begin_aarq_invoke",
        "62474804000010016b2c282a060700118605010101a01f601d80020780a10906 \
         0704000001001a02be0c280a0603883701a0030401016c11a10f02010102013a \
         0407915155101032f4",
        "62804804000010016b802880060700118605010101a080601d80020780a10906 \
         0704000001001a02be0c280a0603883701a0030401010000000000006c80a180 \
         02010102013a0407915155101032f4000000000000",
    ),
    (
        "continue_aare_linked_invoke_result_not_last",
        "6559480220024904000010016b2a2828060700118605010101a01d611b800207 \
         80a109060704000001001a02a203020100a305a1030201006c21a10b0201fe80 \
         01010603883702a712020101300d02013a040800010121436587f9",
        "6580480220024904000010016b802880060700118605010101a080611b800207 \
         80a109060704000001001a02a203020100a305a1030201000000000000006c80 \
         a1800201fe80010106038837020000a780020101308002013a04080001012143 \
         6587f90000000000000000",
    ),
    (
        "end_results_and_error",
        "64284901016c23a212020101300d02013a040800010121436587f9a203020102 \
         a3080201030201013000",
        "64804901016c80a280020101308002013a040800010121436587f900000000a2 \
         800201020000a3800201030201013000000000000000",
    ),
    (
        "end_rejects",
        "642649030102036c1fa4050500800101a406020105810102a406020181820101 \
         a40602017f830104",
        "648049030102036c80a48005008001010000a4800201058101020000a4800201 \
         818201010000a48002017f830104000000000000",
    ),
    (
        "abort_p_abort_cause",
        "67094904000010014a0103",
        "67804904000010014a01030000",
    ),
    (
        "abort_u_abort_abrt",
        "67284904000010016b20281e060700118605010101a0136411800100be0c280a \
         0603883701a003040101",
        "67804904000010016b802880060700118605010101a0806411800100be0c280a \
         0603883701a0030401010000000000000000",
    ),
    (
        "abort_u_abort_aare_rejected",
        "67324904000010016b2a2828060700118605010101a01d611b80020780a10906 \
         0704000001001a02a203020101a305a203020102",
        "67804904000010016b802880060700118605010101a080611b80020780a10906 \
         0704000001001a02a203020101a305a2030201020000000000000000",
    ),
    ("abort_bare", "67034901ff", "67804901ff0000"),
    (
        "unidirectional_audt_invoke",
        "61336b1e281c060700118605010201a011600f80020780a10906070400000100 \
         1a026c11a10f02010002013a0407915155101032f4",
        "61806b802880060700118605010201a080600f80020780a10906070400000100 \
         1a020000000000006c80a18002010002013a0407915155101032f40000000000 \
         00",
    ),
    (
        "begin_aarq_default_version",
        "622148030a0b0c6b1a2818060700118605010101a00d600ba109060704000001 \
         001a02",
        "628048030a0b0c6b802880060700118605010101a080600ba109060704000001 \
         001a020000000000000000",
    ),
];

const CONTEXT: &[u32] = &[0, 4, 0, 0, 1, 0, 26, 2];

fn context() -> &'static Oid {
    Oid::const_new(CONTEXT)
}

fn number() -> Any {
    Any::new(vector("04 07 91 51 55 10 10 32 f4"))
}

fn identity() -> Any {
    Any::new(vector("04 08 00 01 01 21 43 65 87 f9"))
}

/// The user information the script attaches: one EXTERNAL in the made-up
/// abstract syntax 2.999.1 holding the OCTET STRING `04 01 01`.
fn user_information() -> Option<Vec<External>> {
    Some(vec![External::single_asn1_type(
        Oid::const_new(&[2, 999, 1]),
        Any::new(vector("04 01 01")),
    )])
}

fn expected(name: &str) -> TcapMessage {
    match name {
        "begin_aarq_invoke" => TcapMessage::Begin(Begin {
            otid: vec![0x00, 0x00, 0x10, 0x01].into(),
            dialogue_portion: Some(DialoguePortion::from_pdu(&DialoguePdu::Aarq {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: context().into(),
                user_information: user_information(),
            })),
            components: Some(vec![Component::Invoke(Invoke {
                invoke_id: 1,
                linked_id: None,
                operation_code: OperationCode::Local(58),
                parameter: Some(number()),
            })]),
        }),
        "continue_aare_linked_invoke_result_not_last" => TcapMessage::Continue(Continue {
            otid: vec![0x20, 0x02].into(),
            dtid: vec![0x00, 0x00, 0x10, 0x01].into(),
            dialogue_portion: Some(DialoguePortion::aare_accept(context())),
            components: Some(vec![
                Component::Invoke(Invoke {
                    invoke_id: -2,
                    linked_id: Some(1),
                    operation_code: OperationCode::Global(
                        ObjectIdentifier::new(vec![2, 999, 2]).unwrap(),
                    ),
                    parameter: None,
                }),
                Component::ReturnResultNotLast(ReturnResult {
                    invoke_id: 1,
                    result: Some(ReturnResultValue {
                        operation_code: OperationCode::Local(58),
                        parameter: Some(identity()),
                    }),
                }),
            ]),
        }),
        "end_results_and_error" => TcapMessage::End(End {
            dtid: vec![0x01].into(),
            dialogue_portion: None,
            components: Some(vec![
                Component::ReturnResultLast(ReturnResult {
                    invoke_id: 1,
                    result: Some(ReturnResultValue {
                        operation_code: OperationCode::Local(58),
                        parameter: Some(identity()),
                    }),
                }),
                Component::ReturnResultLast(ReturnResult {
                    invoke_id: 2,
                    result: None,
                }),
                Component::ReturnError(ReturnError {
                    invoke_id: 3,
                    error_code: ErrorCode::Local(1),
                    parameter: Some(Any::new(vector("30 00"))),
                }),
            ]),
        }),
        "end_rejects" => TcapMessage::End(End {
            dtid: vec![0x01, 0x02, 0x03].into(),
            dialogue_portion: None,
            components: Some(vec![
                Component::Reject(Reject::general(None, GeneralProblem::MISTYPED_COMPONENT)),
                Component::Reject(Reject::invoke(5, InvokeProblem::MISTYPED_PARAMETER)),
                Component::Reject(Reject::return_result(
                    -127,
                    ReturnResultProblem::RETURN_RESULT_UNEXPECTED,
                )),
                Component::Reject(Reject::return_error(
                    127,
                    ReturnErrorProblem::MISTYPED_PARAMETER,
                )),
            ]),
        }),
        "abort_p_abort_cause" => TcapMessage::Abort(Abort::p_abort(
            vec![0x00, 0x00, 0x10, 0x01].into(),
            PAbortCause::INCORRECT_TRANSACTION_PORTION,
        )),
        "abort_u_abort_abrt" => TcapMessage::Abort(Abort::u_abort(
            vec![0x00, 0x00, 0x10, 0x01].into(),
            Some(DialoguePortion::from_pdu(&DialoguePdu::Abrt {
                abort_source: AbortSource::DialogueServiceUser,
                user_information: user_information(),
            })),
        )),
        "abort_u_abort_aare_rejected" => TcapMessage::Abort(Abort::u_abort(
            vec![0x00, 0x00, 0x10, 0x01].into(),
            Some(DialoguePortion::no_common_dialogue_portion(context())),
        )),
        "abort_bare" => TcapMessage::Abort(Abort::u_abort(vec![0xff].into(), None)),
        "unidirectional_audt_invoke" => TcapMessage::Unidirectional(Unidirectional {
            dialogue_portion: Some(DialoguePortion::audt(context())),
            components: vec![Component::Invoke(Invoke {
                invoke_id: 0,
                linked_id: None,
                operation_code: OperationCode::Local(58),
                parameter: Some(number()),
            })],
        }),
        // The AARQ here has no protocol-version; the member is DEFAULT
        // { version1 }, so it reads as the same value as one that spells the
        // version out.
        "begin_aarq_default_version" => TcapMessage::Begin(Begin {
            otid: vec![0x0a, 0x0b, 0x0c].into(),
            dialogue_portion: Some(DialoguePortion::aarq(context())),
            components: None,
        }),
        other => panic!("no expectation for vector {other}"),
    }
}

/// The dialogue portion as a typed value: two portions are the same when
/// they parse to the same content, whatever length forms and defaults the
/// sender chose.
fn dialogue(message: &TcapMessage) -> Option<DialogueContent> {
    message
        .dialogue_portion()
        .map(|portion| portion.parse().expect("well-formed dialogue portion"))
}

/// The message with its dialogue portion taken out, for comparing the rest.
fn without_dialogue(mut message: TcapMessage) -> TcapMessage {
    match &mut message {
        TcapMessage::Unidirectional(m) => m.dialogue_portion = None,
        TcapMessage::Begin(m) => m.dialogue_portion = None,
        TcapMessage::End(m) => m.dialogue_portion = None,
        TcapMessage::Continue(m) => m.dialogue_portion = None,
        TcapMessage::Abort(m) => {
            if matches!(m.reason, Some(tcap::AbortReason::UAbort(_))) {
                m.reason = None;
            }
        }
    }
    message
}

#[test]
fn foreign_encodings_decode_to_the_values_they_were_built_from() {
    for (name, definite, indefinite) in VECTORS {
        let want = expected(name);
        for (form, hex) in [("definite", definite), ("indefinite", indefinite)] {
            let got = tcap::decode(&vector(hex))
                .unwrap_or_else(|e| panic!("{name} ({form}) does not decode: {e}"));
            assert_eq!(dialogue(&got), dialogue(&want), "{name} ({form}) dialogue");
            assert_eq!(
                without_dialogue(got),
                without_dialogue(want.clone()),
                "{name} ({form})"
            );
        }
    }
}

#[test]
fn the_definite_foreign_encoding_is_what_this_crate_emits() {
    // Two encoders that agree octet for octet. The one vector left out omits
    // the DEFAULT protocol version, which this crate always writes.
    for (name, definite, _) in VECTORS {
        if *name == "begin_aarq_default_version" {
            continue;
        }
        let ours = tcap::encode(&expected(name)).unwrap();
        assert_eq!(hex::encode(ours), hex::encode(vector(definite)), "{name}");
    }
}

#[test]
fn the_refusing_response_reads_back_as_q774_describes_it() {
    // 3.2.3/Q.774, as pyasn1 encoded it.
    let (_, definite, _) = VECTORS
        .iter()
        .find(|(name, _, _)| *name == "abort_u_abort_aare_rejected")
        .unwrap();
    let message = tcap::decode(&vector(definite)).unwrap();
    let pdu = message.dialogue_portion().unwrap().dialogue_pdu().unwrap();
    assert_eq!(
        pdu,
        Some(DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: context().into(),
            result: AssociateResult::RejectedPermanent,
            result_source_diagnostic:
                AssociateSourceDiagnostic::PROVIDER_NO_COMMON_DIALOGUE_PORTION,
            user_information: None,
        })
    );
}
