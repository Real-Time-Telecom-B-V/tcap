//! Every message type and every component type, checked three ways.
//!
//! 1. A byte vector assembled by hand from the ASN.1 of ITU-T Q.773 (06/97)
//!    clause 3.1 and the tag tables of clause 4.2, with the derivation in a
//!    comment. The crate must encode the value to exactly those bytes and
//!    decode those bytes to exactly that value.
//! 2. Wireshark's dissection of the bytes the crate emitted, asserted field by
//!    field, with no malformed or BER error marker anywhere.
//! 3. A decode from bytes this crate did not produce: see
//!    `tests/foreign_vectors.rs`.
//!
//! All values are synthetic. The operation is MAP operation 58, whose
//! argument and result are plain octet strings: a number in the fictional
//! +1 555 01xx range and an identity in the test network 001 01.

mod common;

use common::{dissect, known_answer, vector};
use rasn::types::{Any, ObjectIdentifier, Oid};
use tcap::{
    Abort, AbortSource, Begin, Component, Continue, DialoguePortion, End, ErrorCode,
    GeneralProblem, Invoke, InvokeProblem, OperationCode, PAbortCause, Reject, ReturnError,
    ReturnErrorProblem, ReturnResult, ReturnResultProblem, ReturnResultValue, TcapError,
    TcapMessage, Unidirectional,
};

/// `{ itu-t(0) identified-organization(4) etsi(0) mobileDomain(0)
/// gsm-Network(1) ac-Id(0) imsiRetrieval(26) version2(2) }`.
/// Content octets: `04 00 00 01 00 1a 02` (first two arcs 0*40 + 4 = 04).
const CONTEXT: &[u32] = &[0, 4, 0, 0, 1, 0, 26, 2];

fn context() -> &'static Oid {
    Oid::const_new(CONTEXT)
}

/// Argument of operation 58: `04 07` OCTET STRING, `91` international number
/// in the E.164 plan, digits 1 555 010 1234 in swapped-nibble BCD with filler.
fn number() -> Any {
    Any::new(vector("04 07 91 51 55 10 10 32 f4"))
}

/// Result of operation 58: `04 08` OCTET STRING, identity 001 01 0123456789
/// in swapped-nibble BCD with filler.
fn identity() -> Any {
    Any::new(vector("04 08 00 01 01 21 43 65 87 f9"))
}

fn otid() -> tcap::TransactionId {
    vec![0x00, 0x00, 0x10, 0x01].into()
}

fn dtid() -> tcap::TransactionId {
    vec![0x00, 0x00, 0x20, 0x02].into()
}

fn invoke(parameter: Option<Any>) -> Component {
    Component::Invoke(Invoke {
        invoke_id: 1,
        linked_id: None,
        operation_code: OperationCode::Local(58),
        parameter,
    })
}

fn end(component: Component) -> TcapMessage {
    TcapMessage::End(End {
        dtid: dtid(),
        dialogue_portion: None,
        components: Some(vec![component]),
    })
}

// ── Message types ───────────────────────────────────────────────────────────

#[test]
fn begin_with_an_invoke() {
    let message = TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: None,
        components: Some(vec![invoke(Some(number()))]),
    });
    let bytes = known_answer(
        &message,
        "62 19                      -- Begin [APPLICATION 2] constructed, 25 octets
            48 04 00 00 10 01       -- otid [APPLICATION 8] primitive, 4 octets
            6c 11                   -- ComponentPortion [APPLICATION 12] constructed, 17
               a1 0f                -- invoke [1] constructed, 15
                  02 01 01          -- invokeID INTEGER 1
                  02 01 3a          -- operationCode localValue INTEGER 58
                  04 07 91 51 55 10 10 32 f4   -- parameter",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.begin_element")
            .hex("tcap.otid", "00001001")
            .absent("tcap.dtid")
            .absent("tcap.oid")
            .show("tcap.components", "1")
            .present("gsm_old.invoke_element")
            .show("gsm_old.invokeID", "1")
            .absent("gsm_old.linkedID")
            .show("gsm_old.localValue", "58")
            .show("e164.msisdn", "15550101234");
    }
}

#[test]
fn begin_with_a_dialogue_request() {
    let message = TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: Some(DialoguePortion::aarq(context())),
        components: Some(vec![invoke(Some(number()))]),
    });
    let bytes = known_answer(
        &message,
        "62 39                      -- Begin, 6 + 32 + 19 = 57 octets
            48 04 00 00 10 01       -- otid
            6b 1e                   -- DialoguePortion [APPLICATION 11] constructed, 30
               28 1c                -- EXTERNAL [UNIVERSAL 8] constructed, 9 + 19 = 28
                  06 07 00 11 86 05 01 01 01   -- direct-reference 0.0.17.773.1.1.1
                                    --   (0*40+0 = 00, 17 = 11, 773 = 6*128+5 = 86 05)
                  a0 11             -- single-ASN1-type [0] constructed, 17
                     60 0f          -- AARQ-apdu [APPLICATION 0] constructed, 4 + 11 = 15
                        80 02 07 80             -- protocol-version [0] IMPLICIT BIT STRING,
                                    --   7 unused bits, bit 0 (version1) set
                        a1 09       -- application-context-name [1] explicit, 9
                           06 07 04 00 00 01 00 1a 02   -- OBJECT IDENTIFIER
            6c 11                   -- ComponentPortion, 17
               a1 0f 02 01 01 02 01 3a 04 07 91 51 55 10 10 32 f4   -- invoke as above",
    );
    if let Some(d) = dissect(&bytes) {
        d.hex("tcap.otid", "00001001")
            .show("tcap.oid", "0.0.17.773.1.1.1")
            .present("tcap.dialogueRequest_element")
            .show("tcap.AARQ.protocol.version.version1", "True")
            .show("tcap.aarq_application_context_name", "0.4.0.0.1.0.26.2")
            .absent("tcap.aarq_user_information")
            .show("gsm_old.invokeID", "1")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn begin_without_components() {
    let message = TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: None,
        components: None,
    });
    let bytes = known_answer(
        &message,
        "62 06  48 04 00 00 10 01   -- Begin holding the otid only",
    );
    if let Some(d) = dissect(&bytes) {
        d.hex("tcap.otid", "00001001").absent("tcap.components");
    }
}

#[test]
fn continue_with_a_dialogue_response_and_a_linked_invoke() {
    let message = TcapMessage::Continue(Continue {
        otid: otid(),
        dtid: dtid(),
        dialogue_portion: Some(DialoguePortion::aare_accept(context())),
        components: Some(vec![Component::Invoke(Invoke {
            invoke_id: 2,
            linked_id: Some(1),
            operation_code: OperationCode::Local(58),
            parameter: None,
        })]),
    });
    let bytes = known_answer(
        &message,
        "65 45                      -- Continue [APPLICATION 5] constructed, 6+6+44+13 = 69
            48 04 00 00 10 01       -- otid
            49 04 00 00 20 02       -- dtid [APPLICATION 9] primitive
            6b 2a                   -- DialoguePortion, 42
               28 28                -- EXTERNAL, 9 + 31 = 40
                  06 07 00 11 86 05 01 01 01   -- dialogue-as
                  a0 1d             -- single-ASN1-type, 29
                     61 1b          -- AARE-apdu [APPLICATION 1] constructed, 4+11+5+7 = 27
                        80 02 07 80            -- protocol-version
                        a1 09 06 07 04 00 00 01 00 1a 02   -- application-context-name
                        a2 03 02 01 00         -- result [2] explicit INTEGER accepted (0)
                        a3 05                  -- result-source-diagnostic [3] explicit
                           a1 03 02 01 00      -- dialogue-service-user [1] explicit
                                               --   INTEGER null (0)
            6c 0b                   -- ComponentPortion, 11
               a1 09                -- invoke, 9
                  02 01 02          -- invokeID 2
                  80 01 01          -- linkedID [0] IMPLICIT INTEGER 1
                  02 01 3a          -- operationCode localValue 58",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.continue_element")
            .hex("tcap.otid", "00001001")
            .hex("tcap.dtid", "00002002")
            .present("tcap.dialogueResponse_element")
            .show("tcap.AARE.protocol.version.version1", "True")
            .show("tcap.aare_application_context_name", "0.4.0.0.1.0.26.2")
            .show("tcap.result", "0")
            .show("tcap.dialogue_service_user", "0")
            .absent("tcap.dialogue_service_provider")
            .show("gsm_old.invokeID", "2")
            .show("gsm_old.linkedID", "1")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn end_with_a_return_result_last() {
    let message = end(Component::ReturnResultLast(ReturnResult {
        invoke_id: 1,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(58),
            parameter: Some(identity()),
        }),
    }));
    let bytes = known_answer(
        &message,
        "64 1c                      -- End [APPLICATION 4] constructed, 6 + 22 = 28
            49 04 00 00 20 02       -- dtid
            6c 14                   -- ComponentPortion, 20
               a2 12                -- returnResultLast [2] constructed, 3 + 15 = 18
                  02 01 01          -- invokeID 1
                  30 0d             -- result SEQUENCE, 3 + 10 = 13
                     02 01 3a       -- operationCode localValue 58
                     04 08 00 01 01 21 43 65 87 f9   -- parameter",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.end_element")
            .hex("tcap.dtid", "00002002")
            .absent("tcap.otid")
            .present("gsm_old.returnResultLast_element")
            .show("gsm_old.invokeID", "1")
            .present("gsm_old.resultretres_element")
            .show("gsm_old.localValue", "58")
            .show("e212.imsi", "001010123456789");
    }
}

#[test]
fn unidirectional_with_a_dialogue_and_an_invoke() {
    let message = TcapMessage::Unidirectional(Unidirectional {
        dialogue_portion: Some(DialoguePortion::audt(context())),
        components: vec![Component::Invoke(Invoke {
            invoke_id: 0,
            linked_id: None,
            operation_code: OperationCode::Local(58),
            parameter: Some(number()),
        })],
    });
    let bytes = known_answer(
        &message,
        "61 33                      -- Unidirectional [APPLICATION 1] constructed, 32 + 19 = 51
            6b 1e                   -- DialoguePortion, 30
               28 1c                -- EXTERNAL, 28
                  06 07 00 11 86 05 01 02 01   -- direct-reference 0.0.17.773.1.2.1,
                                    --   unidialogue-as (2)
                  a0 11             -- single-ASN1-type
                     60 0f          -- AUDT-apdu [APPLICATION 0] constructed
                        80 02 07 80            -- protocol-version
                        a1 09 06 07 04 00 00 01 00 1a 02   -- application-context-name
            6c 11                   -- ComponentPortion
               a1 0f 02 01 00 02 01 3a 04 07 91 51 55 10 10 32 f4   -- invoke, invokeID 0",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.unidirectional_element")
            .absent("tcap.otid")
            .absent("tcap.dtid")
            // Wireshark reads the abstract syntax name and then shows the
            // AUDT under its AARQ field names: the two PDUs have the same
            // tag and the same members.
            .show("tcap.oid", "0.0.17.773.1.2.1")
            .show("tcap.AARQ.protocol.version.version1", "True")
            .show("tcap.aarq_application_context_name", "0.4.0.0.1.0.26.2")
            .show("gsm_old.invokeID", "0")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn abort_with_each_p_abort_cause() {
    // Table 12/Q.773.
    for (cause, value, name) in [
        (
            PAbortCause::UNRECOGNIZED_MESSAGE_TYPE,
            "00",
            "unrecognizedMessageType",
        ),
        (
            PAbortCause::UNRECOGNIZED_TRANSACTION_ID,
            "01",
            "unrecognizedTransactionID",
        ),
        (
            PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION,
            "02",
            "badlyFormattedTransactionPortion",
        ),
        (
            PAbortCause::INCORRECT_TRANSACTION_PORTION,
            "03",
            "incorrectTransactionPortion",
        ),
        (PAbortCause::RESOURCE_LIMITATION, "04", "resourceLimitation"),
    ] {
        assert_eq!(cause.name(), Some(name));
        let message = TcapMessage::Abort(Abort::p_abort(dtid(), cause));
        let bytes = known_answer(
            &message,
            &format!(
                "67 09                   -- Abort [APPLICATION 7] constructed, 6 + 3 = 9
                    49 04 00 00 20 02    -- dtid
                    4a 01 {value}        -- P-AbortCause [APPLICATION 10] IMPLICIT INTEGER"
            ),
        );
        if let Some(d) = dissect(&bytes) {
            d.present("tcap.abort_element")
                .hex("tcap.dtid", "00002002")
                .show("tcap.reason", "10")
                .hex("tcap.p_abortCause", value)
                .absent("tcap.oid");
            let label = &d
                .fields
                .iter()
                .find(|f| f.name == "tcap.p_abortCause")
                .unwrap()
                .showname;
            assert!(label.contains(name), "{label}");
        }
    }
}

#[test]
fn abort_with_user_abort_information() {
    let message = TcapMessage::Abort(Abort::u_abort(
        dtid(),
        Some(DialoguePortion::abrt(AbortSource::DialogueServiceUser)),
    ));
    let bytes = known_answer(
        &message,
        "67 1a                      -- Abort, 6 + 20 = 26
            49 04 00 00 20 02       -- dtid
            6b 12                   -- u-abortCause DialoguePortion [APPLICATION 11], 18
               28 10                -- EXTERNAL, 9 + 7 = 16
                  06 07 00 11 86 05 01 01 01   -- dialogue-as
                  a0 05             -- single-ASN1-type
                     64 03          -- ABRT-apdu [APPLICATION 4] constructed
                        80 01 00    -- abort-source [0] IMPLICIT INTEGER
                                    --   dialogue-service-user (0)",
    );
    if let Some(d) = dissect(&bytes) {
        d.hex("tcap.dtid", "00002002")
            .absent("tcap.p_abortCause")
            .show("tcap.reason", "11")
            .show("tcap.oid", "0.0.17.773.1.1.1")
            .present("tcap.dialogueAbort_element")
            .show("tcap.abort_source", "0");
    }
}

#[test]
fn abort_with_no_reason() {
    let message = TcapMessage::Abort(Abort::u_abort(dtid(), None));
    let bytes = known_answer(
        &message,
        "67 06  49 04 00 00 20 02   -- Abort holding the dtid only",
    );
    if let Some(d) = dissect(&bytes) {
        d.hex("tcap.dtid", "00002002")
            .absent("tcap.reason")
            .absent("tcap.p_abortCause");
    }
}

// ── Component types ─────────────────────────────────────────────────────────

#[test]
fn invoke_without_a_parameter() {
    let message = end(invoke(None));
    let bytes = known_answer(
        &message,
        "64 10  49 04 00 00 20 02   -- End, 6 + 10 = 16
            6c 08
               a1 06                -- invoke, 6
                  02 01 01          -- invokeID 1
                  02 01 3a          -- operationCode localValue 58",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("gsm_old.invokeID", "1")
            .absent("gsm_old.linkedID")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn invoke_with_negative_ids_a_linked_id_and_a_parameter() {
    // InvokeIdType ::= INTEGER (-128..127): one content octet, two's
    // complement. -128 is 80, -1 is ff.
    let message = end(Component::Invoke(Invoke {
        invoke_id: -128,
        linked_id: Some(-1),
        operation_code: OperationCode::Local(58),
        parameter: Some(number()),
    }));
    let bytes = known_answer(
        &message,
        "64 1c  49 04 00 00 20 02   -- End, 6 + 22 = 28
            6c 14
               a1 12                -- invoke, 3 + 3 + 3 + 9 = 18
                  02 01 80          -- invokeID -128
                  80 01 ff          -- linkedID [0] -1
                  02 01 3a          -- operationCode localValue 58
                  04 07 91 51 55 10 10 32 f4   -- parameter",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("gsm_old.invokeID", "-128")
            .show("gsm_old.linkedID", "-1")
            .show("gsm_old.localValue", "58")
            .show("e164.msisdn", "15550101234");
    }
}

#[test]
fn invoke_with_a_global_operation_code() {
    // 2.999.2 under the example arc of X.660: first subidentifier
    // 2*40 + 999 = 1079 = 8*128 + 55, so 88 37; then 02.
    let message = end(Component::Invoke(Invoke {
        invoke_id: 1,
        linked_id: None,
        operation_code: OperationCode::Global(ObjectIdentifier::new(vec![2, 999, 2]).unwrap()),
        parameter: None,
    }));
    let bytes = known_answer(
        &message,
        "64 12  49 04 00 00 20 02   -- End, 6 + 12 = 18
            6c 0a
               a1 08                -- invoke, 3 + 5 = 8
                  02 01 01          -- invokeID 1
                  06 03 88 37 02    -- operationCode globalValue OBJECT IDENTIFIER",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("gsm_old.invokeID", "1")
            .show("gsm_old.globalValue", "2.999.2")
            .absent("gsm_old.localValue");
    }
}

#[test]
fn return_result_last_and_not_last_without_a_result() {
    for (last, tag, element) in [
        (true, "a2", "gsm_old.returnResultLast_element"),
        (false, "a7", "gsm_old.returnResultNotLast_element"),
    ] {
        let result = ReturnResult {
            invoke_id: 1,
            result: None,
        };
        let message = end(if last {
            Component::ReturnResultLast(result)
        } else {
            Component::ReturnResultNotLast(result)
        });
        let bytes = known_answer(
            &message,
            &format!(
                "64 0d  49 04 00 00 20 02   -- End, 6 + 7 = 13
                    6c 05
                       {tag} 03             -- returnResultLast [2] / NotLast [7]
                          02 01 01          -- invokeID 1"
            ),
        );
        if let Some(d) = dissect(&bytes) {
            d.present(element)
                .show("gsm_old.invokeID", "1")
                .absent("gsm_old.resultretres_element");
        }
    }
}

#[test]
fn return_result_not_last_with_a_result() {
    let message = end(Component::ReturnResultNotLast(ReturnResult {
        invoke_id: 1,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(58),
            parameter: Some(identity()),
        }),
    }));
    let bytes = known_answer(
        &message,
        "64 1c  49 04 00 00 20 02
            6c 14
               a7 12                -- returnResultNotLast [7] constructed, 18
                  02 01 01          -- invokeID 1
                  30 0d             -- result SEQUENCE
                     02 01 3a       -- operationCode localValue 58
                     04 08 00 01 01 21 43 65 87 f9   -- parameter",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("gsm_old.returnResultNotLast_element")
            .show("gsm_old.invokeID", "1")
            .show("gsm_old.localValue", "58");
    }
}

#[test]
fn return_error_without_and_with_a_parameter() {
    let bare = end(Component::ReturnError(ReturnError {
        invoke_id: 1,
        error_code: ErrorCode::Local(1),
        parameter: None,
    }));
    let bytes = known_answer(
        &bare,
        "64 10  49 04 00 00 20 02   -- End, 6 + 10 = 16
            6c 08
               a3 06                -- returnError [3] constructed
                  02 01 01          -- invokeID 1
                  02 01 01          -- errorCode localValue 1",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("gsm_old.returnError_element")
            .show("gsm_old.invokeID", "1")
            .show("gsm_old.localValue", "1");
    }

    // Error 1 of MAP takes a SEQUENCE; 0a 01 00 is its diagnostic,
    // ENUMERATED 0.
    let with_parameter = end(Component::ReturnError(ReturnError {
        invoke_id: 1,
        error_code: ErrorCode::Local(1),
        parameter: Some(Any::new(vector("30 03 0a 01 00"))),
    }));
    let bytes = known_answer(
        &with_parameter,
        "64 15  49 04 00 00 20 02   -- End, 6 + 15 = 21
            6c 0d
               a3 0b                -- returnError, 3 + 3 + 5 = 11
                  02 01 01          -- invokeID 1
                  02 01 01          -- errorCode localValue 1
                  30 03 0a 01 00    -- parameter",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("gsm_old.invokeID", "1")
            .show("gsm_old.localValue", "1")
            .show("gsm_map.er.unknownSubscriberDiagnostic", "0");
    }
}

#[test]
fn return_error_with_a_global_error_code() {
    let message = end(Component::ReturnError(ReturnError {
        invoke_id: 1,
        error_code: ErrorCode::Global(ObjectIdentifier::new(vec![2, 999, 3]).unwrap()),
        parameter: None,
    }));
    let bytes = known_answer(
        &message,
        "64 12  49 04 00 00 20 02
            6c 0a
               a3 08
                  02 01 01          -- invokeID 1
                  06 03 88 37 03    -- errorCode globalValue 2.999.3",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("gsm_old.invokeID", "1")
            .show("gsm_old.globalValue", "2.999.3");
    }
}

#[test]
fn reject_with_each_problem_class() {
    // Table 25/Q.773: general [0], invoke [1], return result [2], return
    // error [3], each an IMPLICIT INTEGER.
    let cases: [(Reject, &str, &str, &str); 4] = [
        (
            Reject::general(Some(5), GeneralProblem::MISTYPED_COMPONENT),
            "80 01 01",
            "gsm_old.generalProblem",
            "1",
        ),
        (
            Reject::invoke(5, InvokeProblem::UNRECOGNIZED_LINKED_ID),
            "81 01 05",
            "gsm_old.invokeProblem",
            "5",
        ),
        (
            Reject::return_result(5, ReturnResultProblem::MISTYPED_PARAMETER),
            "82 01 02",
            "gsm_old.returnResultProblem",
            "2",
        ),
        (
            Reject::return_error(5, ReturnErrorProblem::MISTYPED_PARAMETER),
            "83 01 04",
            "gsm_old.returnErrorProblem",
            "4",
        ),
    ];
    for (reject, problem, field, value) in cases {
        let bytes = known_answer(
            &end(Component::Reject(reject)),
            &format!(
                "64 10  49 04 00 00 20 02   -- End, 6 + 10 = 16
                    6c 08
                       a4 06                -- reject [4] constructed
                          02 01 05          -- invokeID derivable INTEGER 5
                          {problem}         -- problem"
            ),
        );
        if let Some(d) = dissect(&bytes) {
            d.present("gsm_old.reject_element")
                .show("gsm_old.derivable", "5")
                .absent("gsm_old.not_derivable_element")
                .show(field, value);
        }
    }
}

#[test]
fn reject_with_an_invoke_id_that_is_not_derivable() {
    // Table 18/Q.773, note a: "If the Invoke ID is not available, Universal
    // Null (see Table 21) with length = 0 should be used."
    let reject = Reject::general(None, GeneralProblem::BADLY_STRUCTURED_COMPONENT);
    let bytes = known_answer(
        &end(Component::Reject(reject)),
        "64 0f  49 04 00 00 20 02   -- End, 6 + 9 = 15
            6c 07
               a4 05                -- reject
                  05 00             -- invokeID not-derivable NULL
                  80 01 02          -- generalProblem badlyStructuredComponent (2)",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("gsm_old.not_derivable_element")
            .absent("gsm_old.derivable")
            .show("gsm_old.generalProblem", "2");
    }
}

#[test]
fn every_named_problem_code_has_the_value_of_its_table() {
    // Tables 26 to 29/Q.773.
    assert_eq!(GeneralProblem::UNRECOGNIZED_COMPONENT.value(), 0);
    assert_eq!(GeneralProblem::MISTYPED_COMPONENT.value(), 1);
    assert_eq!(GeneralProblem::BADLY_STRUCTURED_COMPONENT.value(), 2);
    assert_eq!(InvokeProblem::DUPLICATE_INVOKE_ID.value(), 0);
    assert_eq!(InvokeProblem::UNRECOGNIZED_OPERATION.value(), 1);
    assert_eq!(InvokeProblem::MISTYPED_PARAMETER.value(), 2);
    assert_eq!(InvokeProblem::RESOURCE_LIMITATION.value(), 3);
    assert_eq!(InvokeProblem::INITIATING_RELEASE.value(), 4);
    assert_eq!(InvokeProblem::UNRECOGNIZED_LINKED_ID.value(), 5);
    assert_eq!(InvokeProblem::LINKED_RESPONSE_UNEXPECTED.value(), 6);
    assert_eq!(InvokeProblem::UNEXPECTED_LINKED_OPERATION.value(), 7);
    assert_eq!(ReturnResultProblem::UNRECOGNIZED_INVOKE_ID.value(), 0);
    assert_eq!(ReturnResultProblem::RETURN_RESULT_UNEXPECTED.value(), 1);
    assert_eq!(ReturnResultProblem::MISTYPED_PARAMETER.value(), 2);
    assert_eq!(ReturnErrorProblem::UNRECOGNIZED_INVOKE_ID.value(), 0);
    assert_eq!(ReturnErrorProblem::RETURN_ERROR_UNEXPECTED.value(), 1);
    assert_eq!(ReturnErrorProblem::UNRECOGNIZED_ERROR.value(), 2);
    assert_eq!(ReturnErrorProblem::UNEXPECTED_ERROR.value(), 3);
    assert_eq!(ReturnErrorProblem::MISTYPED_PARAMETER.value(), 4);
    // A value without a name is still a value of the type.
    assert_eq!(GeneralProblem(9).name(), None);
    assert_eq!(GeneralProblem(9).to_string(), "9");
    assert_eq!(
        GeneralProblem::MISTYPED_COMPONENT.to_string(),
        "mistypedComponent(1)"
    );
}

#[test]
fn several_components_in_one_message() {
    let message = TcapMessage::Continue(Continue {
        otid: otid(),
        dtid: dtid(),
        dialogue_portion: None,
        components: Some(vec![
            invoke(None),
            Component::ReturnResultLast(ReturnResult {
                invoke_id: 2,
                result: None,
            }),
            Component::Reject(Reject::invoke(3, InvokeProblem::DUPLICATE_INVOKE_ID)),
        ]),
    });
    let bytes = known_answer(
        &message,
        "65 23                      -- Continue, 6 + 6 + 23 = 35
            48 04 00 00 10 01
            49 04 00 00 20 02
            6c 15                   -- ComponentPortion, 8 + 5 + 8 = 21
               a1 06 02 01 01 02 01 3a   -- invoke 1, operation 58
               a2 03 02 01 02            -- returnResultLast 2
               a4 06 02 01 03 81 01 00   -- reject 3, invokeProblem duplicateInvokeID (0)",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("tcap.components", "3")
            .show_all("gsm_old.invokeID", &["1", "2"])
            .show("gsm_old.derivable", "3")
            .show("gsm_old.invokeProblem", "0");
    }
}

// ── Transaction ID lengths ──────────────────────────────────────────────────

#[test]
fn transaction_ids_of_one_to_four_octets() {
    // "The length of a Transaction ID is 1 to 4 octets" (4.2.1.3/Q.773).
    for id in ["aa", "aa bb", "aa bb cc", "aa bb cc dd"] {
        let octets = vector(id);
        let length = octets.len();
        let message = TcapMessage::Continue(Continue {
            otid: octets.clone().into(),
            dtid: octets.clone().into(),
            dialogue_portion: None,
            components: None,
        });
        let bytes = known_answer(
            &message,
            &format!(
                "65 {:02x}               -- Continue, two IDs of 2 + {length} octets each
                    48 {length:02x} {id} -- otid
                    49 {length:02x} {id} -- dtid",
                2 * (2 + length)
            ),
        );
        if let Some(d) = dissect(&bytes) {
            let compact = hex::encode(&octets);
            d.hex("tcap.otid", &compact).hex("tcap.dtid", &compact);
        }
    }
}

#[test]
fn a_transaction_id_of_another_length_is_not_encoded() {
    for length in [0usize, 5, 8] {
        let id: tcap::TransactionId = vec![0xaa; length].into();
        for message in [
            TcapMessage::Begin(Begin {
                otid: id.clone(),
                dialogue_portion: None,
                components: None,
            }),
            TcapMessage::End(End {
                dtid: id.clone(),
                dialogue_portion: None,
                components: None,
            }),
            TcapMessage::Abort(Abort::u_abort(id.clone(), None)),
        ] {
            let error = tcap::encode(&message).unwrap_err();
            assert!(
                matches!(&error, TcapError::InvalidMessage(m) if m.contains("1 to 4")),
                "{error}"
            );
        }
    }
}

// ── What the encoder refuses ────────────────────────────────────────────────

fn refused(message: &TcapMessage) -> String {
    match tcap::encode(message) {
        Err(TcapError::InvalidMessage(detail)) => detail,
        other => panic!("encoded a message Q.773 does not allow: {other:?}"),
    }
}

#[test]
fn an_empty_component_portion_is_not_encoded() {
    // ComponentPortion ::= SEQUENCE SIZE (1..MAX) OF Component.
    let detail = refused(&TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: None,
        components: Some(vec![]),
    }));
    assert!(detail.contains("at least one component"), "{detail}");

    // In a Unidirectional the component portion is not OPTIONAL.
    refused(&TcapMessage::Unidirectional(Unidirectional {
        dialogue_portion: None,
        components: vec![],
    }));
}

#[test]
fn a_result_sequence_without_a_parameter_is_not_encoded() {
    // result SEQUENCE { operationCode OPERATION, parameter ANY DEFINED BY
    // operationCode }: the parameter is not OPTIONAL.
    let detail = refused(&end(Component::ReturnResultLast(ReturnResult {
        invoke_id: 1,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(58),
            parameter: None,
        }),
    })));
    assert!(detail.contains("no parameter"), "{detail}");
}

#[test]
fn a_parameter_that_is_not_one_element_is_not_encoded() {
    for parameter in ["04 05 01", "04 01 01 04 01 02", ""] {
        let detail = refused(&end(invoke(Some(Any::new(vector(parameter))))));
        assert!(detail.contains("parameter"), "{detail}");
    }
}

#[test]
fn a_malformed_dialogue_portion_is_not_encoded() {
    let detail = refused(&TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: Some(DialoguePortion {
            external: Any::new(vector("30 03 02 01 00")),
        }),
        components: None,
    }));
    assert!(detail.contains("EXTERNAL"), "{detail}");
}
