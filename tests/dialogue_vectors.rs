//! The dialogue portion against ITU-T Q.773 (06/97) clauses 3.2 and 4.2.3:
//! hand-assembled vectors, Wireshark's reading of what the crate emits, and
//! the three outcomes of reading a portion: a PDU, something well-formed the
//! crate does not model, and malformed.
//!
//! All values are synthetic. 2.999.x is the example arc of X.660.

mod common;

use common::{dissect, known_answer, vector};
use rasn::types::{Any, Oid};
use tcap::{
    Abort, AbortSource, AssociateResult, AssociateSourceDiagnostic, Begin, DialogueContent,
    DialoguePdu, DialoguePortion, External, ExternalEncoding, ProtocolVersion, TcapMessage,
};

/// `0.4.0.0.1.0.26.2`, content octets `04 00 00 01 00 1a 02`.
const CONTEXT: &[u32] = &[0, 4, 0, 0, 1, 0, 26, 2];

fn context() -> &'static Oid {
    Oid::const_new(CONTEXT)
}

fn otid() -> tcap::TransactionId {
    vec![0x00, 0x00, 0x10, 0x01].into()
}

/// One EXTERNAL in the abstract syntax 2.999.1 holding OCTET STRING `01`:
/// `28 0a  06 03 88 37 01  a0 03 04 01 01`.
fn user_information() -> Option<Vec<External>> {
    Some(vec![External::single_asn1_type(
        Oid::const_new(&[2, 999, 1]),
        Any::new(vec![0x04, 0x01, 0x01]),
    )])
}

fn begin(portion: DialoguePortion) -> TcapMessage {
    TcapMessage::Begin(Begin {
        otid: otid(),
        dialogue_portion: Some(portion),
        components: None,
    })
}

fn abort(portion: DialoguePortion) -> TcapMessage {
    TcapMessage::Abort(Abort::u_abort(otid(), Some(portion)))
}

// ── Hand-assembled vectors and Wireshark ────────────────────────────────────

/// `map-DialogueAS` of 3GPP TS 29.002, `0.4.0.0.1.1.1.1`, the abstract syntax
/// MAP puts in the user information: content octets `04 00 00 01 01 01 01`.
/// Wireshark knows it, so it reads the user information to the end.
const MAP_DIALOGUE_AS: &[u32] = &[0, 4, 0, 0, 1, 1, 1, 1];

#[test]
fn aarq_with_user_information() {
    // The user information is a MAP-OpenInfo with no members: map-open [0]
    // IMPLICIT SEQUENCE {}, a0 00.
    let portion = DialoguePortion::from_pdu(&DialoguePdu::Aarq {
        protocol_version: ProtocolVersion::Version1,
        application_context_name: context().into(),
        user_information: Some(vec![External::single_asn1_type(
            Oid::const_new(MAP_DIALOGUE_AS),
            Any::new(vec![0xa0, 0x00]),
        )]),
    });
    let bytes = known_answer(
        &begin(portion),
        "62 37                      -- Begin, 6 + 49 = 55
            48 04 00 00 10 01
            6b 2f                   -- DialoguePortion [APPLICATION 11], 47
               28 2d                -- EXTERNAL, 9 + 36 = 45
                  06 07 00 11 86 05 01 01 01   -- dialogue-as 0.0.17.773.1.1.1
                  a0 22             -- single-ASN1-type [0], 34
                     60 20          -- AARQ-apdu [APPLICATION 0], 4 + 11 + 17 = 32
                        80 02 07 80            -- protocol-version { version1 }
                        a1 09                  -- application-context-name [1] explicit
                           06 07 04 00 00 01 00 1a 02
                        be 0f                  -- user-information [30] IMPLICIT
                                               --   SEQUENCE OF: class context,
                                               --   constructed, number 30 = 1e
                           28 0d               -- EXTERNAL, 9 + 4 = 13
                              06 07 04 00 00 01 01 01 01   -- direct-reference
                              a0 02 a0 00      -- single-ASN1-type { map-open }",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("tcap.oid", "0.0.17.773.1.1.1")
            .present("tcap.dialogueRequest_element")
            .show("tcap.AARQ.protocol.version.version1", "True")
            .show("tcap.aarq_application_context_name", "0.4.0.0.1.0.26.2")
            .show("tcap.aarq_user_information", "1")
            .present("tcap.aarq_user_information_item_element")
            .show("ber.direct_reference", "0.4.0.0.1.1.1.1")
            .present("gsm_map.dialogue.map_open_element");
    }
}

#[test]
fn aare_refusing_the_application_context() {
    // 3.2.1.2/Q.774: for "application-context-name-not-supported" the AARE
    // has result reject-permanent and travels in an Abort.
    let portion = DialoguePortion::aare_reject(
        context(),
        AssociateSourceDiagnostic::USER_APPLICATION_CONTEXT_NAME_NOT_SUPPORTED,
    );
    let bytes = known_answer(
        &abort(portion),
        "67 32                      -- Abort, 6 + 44 = 50
            49 04 00 00 10 01
            6b 2a
               28 28
                  06 07 00 11 86 05 01 01 01
                  a0 1d
                     61 1b          -- AARE-apdu [APPLICATION 1], 4 + 11 + 5 + 7 = 27
                        80 02 07 80
                        a1 09 06 07 04 00 00 01 00 1a 02
                        a2 03 02 01 01         -- result reject-permanent (1)
                        a3 05                  -- result-source-diagnostic
                           a1 03 02 01 02      -- dialogue-service-user
                                               --   application-context-name-not-supported (2)",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.dialogueResponse_element")
            .show("tcap.aare_application_context_name", "0.4.0.0.1.0.26.2")
            .show("tcap.result", "1")
            .show("tcap.dialogue_service_user", "2")
            .absent("tcap.dialogue_service_provider");
    }
}

#[test]
fn aare_for_no_common_dialogue_portion() {
    // 3.2.3/Q.774: "protocol-version = version 1; application-context-name =
    // the one received in the AARQ APDU; result = reject (permanent);
    // result-source-diagnostic = dialogue-service-provider
    // (no-common-dialogue-version); user-information = absent."
    let bytes = known_answer(
        &abort(DialoguePortion::no_common_dialogue_portion(context())),
        "67 32
            49 04 00 00 10 01
            6b 2a
               28 28
                  06 07 00 11 86 05 01 01 01
                  a0 1d
                     61 1b
                        80 02 07 80
                        a1 09 06 07 04 00 00 01 00 1a 02
                        a2 03 02 01 01         -- result reject-permanent (1)
                        a3 05
                           a2 03 02 01 02      -- dialogue-service-provider [2]
                                               --   no-common-dialogue-portion (2)",
    );
    if let Some(d) = dissect(&bytes) {
        d.show("tcap.result", "1")
            .show("tcap.dialogue_service_provider", "2")
            .absent("tcap.dialogue_service_user")
            .absent("tcap.aare_user_information");
    }
}

#[test]
fn abrt_from_the_user_with_user_information() {
    // The user information is a MAP-UserAbortInfo: map-userAbort [4] IMPLICIT
    // SEQUENCE { userSpecificReason [0] IMPLICIT NULL }, a4 02 80 00.
    let portion = DialoguePortion::from_pdu(&DialoguePdu::Abrt {
        abort_source: AbortSource::DialogueServiceUser,
        user_information: Some(vec![External::single_asn1_type(
            Oid::const_new(MAP_DIALOGUE_AS),
            Any::new(vec![0xa4, 0x02, 0x80, 0x00]),
        )]),
    });
    let bytes = known_answer(
        &abort(portion),
        "67 2d                      -- Abort, 6 + 39 = 45
            49 04 00 00 10 01
            6b 25
               28 23                -- EXTERNAL, 9 + 26 = 35
                  06 07 00 11 86 05 01 01 01
                  a0 18
                     64 16          -- ABRT-apdu [APPLICATION 4], 3 + 19 = 22
                        80 01 00    -- abort-source dialogue-service-user (0)
                        be 11       -- user-information
                           28 0f    -- EXTERNAL, 9 + 6 = 15
                              06 07 04 00 00 01 01 01 01
                              a0 04 a4 02 80 00",
    );
    if let Some(d) = dissect(&bytes) {
        d.present("tcap.dialogueAbort_element")
            .show("tcap.abort_source", "0")
            .show("tcap.abrt_user_information", "1")
            .show("ber.direct_reference", "0.4.0.0.1.1.1.1")
            .present("gsm_map.dialogue.map_userAbort_element");
    }
}

#[test]
fn user_information_in_a_syntax_wireshark_does_not_know() {
    // The same AARQ shape with the made-up syntax 2.999.1. Wireshark reads
    // the EXTERNAL and notes only that it has no dissector for the syntax.
    let portion = DialoguePortion::from_pdu(&DialoguePdu::Aarq {
        protocol_version: ProtocolVersion::Version1,
        application_context_name: context().into(),
        user_information: user_information(),
    });
    let bytes = known_answer(
        &begin(portion),
        "62 34  48 04 00 00 10 01
            6b 2c 28 2a
               06 07 00 11 86 05 01 01 01
               a0 1f 60 1d
                  80 02 07 80
                  a1 09 06 07 04 00 00 01 00 1a 02
                  be 0c
                     28 0a
                        06 03 88 37 01   -- direct-reference 2.999.1
                        a0 03 04 01 01   -- single-ASN1-type { OCTET STRING 01 }",
    );
    if let Some(d) = common::dissect_unchecked(&bytes) {
        d.show("tcap.aarq_user_information", "1")
            .show("ber.direct_reference", "2.999.1")
            .present("ber.error.oid_not_implemented");
        assert!(
            !d.problems().iter().any(|p| p.contains("alformed")),
            "{}",
            d.summary()
        );
    }
}

// ── Reading: forms this crate does not emit ─────────────────────────────────

/// Definite-length element, for assembling test inputs.
fn tlv(identifier: &str, content: &[u8]) -> Vec<u8> {
    assert!(content.len() < 128);
    let mut out = vector(identifier);
    out.push(content.len() as u8);
    out.extend_from_slice(content);
    out
}

const DIALOGUE_AS: &str = "06 07 00 11 86 05 01 01 01";
const UNIDIALOGUE_AS: &str = "06 07 00 11 86 05 01 02 01";

/// A dialogue portion: an EXTERNAL naming `syntax` with `pdu` as a
/// single-ASN1-type.
fn portion(syntax: &str, pdu: &str) -> DialoguePortion {
    let mut content = vector(syntax);
    content.extend(tlv("a0", &vector(pdu)));
    DialoguePortion {
        external: Any::new(tlv("28", &content)),
    }
}

#[test]
fn an_absent_protocol_version_is_version1() {
    // DEFAULT { version1 }.
    let read = portion(DIALOGUE_AS, "60 0b  a1 09 06 07 04 00 00 01 00 1a 02")
        .dialogue_pdu()
        .unwrap();
    assert_eq!(
        read,
        Some(DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: context().into(),
            user_information: None,
        })
    );
}

#[test]
fn an_aarq_that_does_not_list_version1_is_read_and_says_so() {
    // 80 02 06 40: six unused bits, bit 1 set, bit 0 (version1) clear.
    // 3.2.3/Q.774 has the component sub-layer answer this one with an AARE
    // (see aare_for_no_common_dialogue_portion above), so it is not a syntax
    // error: the caller needs the application context name from it.
    let read = portion(
        DIALOGUE_AS,
        "60 0f  80 02 06 40  a1 09 06 07 04 00 00 01 00 1a 02",
    )
    .dialogue_pdu()
    .unwrap();
    match read {
        Some(DialoguePdu::Aarq {
            protocol_version,
            application_context_name,
            ..
        }) => {
            assert_eq!(protocol_version, ProtocolVersion::Other(vec![0x06, 0x40]));
            assert_eq!(&*application_context_name, context());
        }
        other => panic!("{other:?}"),
    }

    // Version 1 listed next to another version is version 1.
    let read = portion(
        DIALOGUE_AS,
        "60 0f  80 02 06 c0  a1 09 06 07 04 00 00 01 00 1a 02",
    )
    .dialogue_pdu()
    .unwrap();
    assert!(matches!(
        read,
        Some(DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            ..
        })
    ));
}

#[test]
fn an_aare_that_does_not_list_version1_is_a_syntax_error() {
    // 3.2.3/Q.774: "the receipt of an AARE APDU with the version field set to
    // any value other than "version 1" shall be considered a syntax error and
    // the procedures described in 3.2.2.1 will be followed."
    let error = portion(
        DIALOGUE_AS,
        "61 1b  80 02 06 40  a1 09 06 07 04 00 00 01 00 1a 02
                a2 03 02 01 00  a3 05 a1 03 02 01 00",
    )
    .parse()
    .unwrap_err();
    assert!(error.detail().contains("version1"), "{error}");
}

#[test]
fn a_pdu_carried_octet_aligned_or_as_a_bit_string_is_read() {
    // Tables 33 and 34/Q.773: "The use of the single-ASN.1-type constructor
    // is only one possible encoding." X.690 8.18 has two more.
    let abrt = vector("64 03 80 01 01");
    let want = Some(DialoguePdu::Abrt {
        abort_source: AbortSource::DialogueServiceProvider,
        user_information: None,
    });

    let mut content = vector(DIALOGUE_AS);
    content.extend(tlv("81", &abrt)); // octet-aligned [1] IMPLICIT OCTET STRING
    let octet_aligned = DialoguePortion {
        external: Any::new(tlv("28", &content)),
    };
    assert_eq!(octet_aligned.dialogue_pdu(), Ok(want.clone()));

    let mut bits = vec![0x00]; // no unused bits
    bits.extend(&abrt);
    let mut content = vector(DIALOGUE_AS);
    content.extend(tlv("82", &bits)); // arbitrary [2] IMPLICIT BIT STRING
    let arbitrary = DialoguePortion {
        external: Any::new(tlv("28", &content)),
    };
    assert_eq!(arbitrary.dialogue_pdu(), Ok(want));
}

// ── Reading: well-formed, and not modelled ──────────────────────────────────

#[test]
fn user_information_in_a_user_defined_syntax_is_handed_over() {
    // The Note to Abort in 3.1/Q.773: the u-abortCause "could be either an
    // ABRT APDU or data in some user-defined abstract syntax".
    let external = External::single_asn1_type(
        Oid::const_new(&[2, 999, 1]),
        Any::new(vec![0x04, 0x01, 0x01]),
    );
    let portion = DialoguePortion::from_external(&external);
    assert_eq!(
        portion.external.as_bytes(),
        &vector("28 0a  06 03 88 37 01  a0 03 04 01 01")[..]
    );
    assert_eq!(portion.parse(), Ok(DialogueContent::Unmodelled(external)));
    assert_eq!(portion.dialogue_pdu(), Ok(None));

    // It travels in a message and comes back the same.
    let message = abort(portion.clone());
    let bytes = tcap::encode(&message).unwrap();
    assert_eq!(tcap::decode(&bytes).unwrap(), message);
}

#[test]
fn the_release_pdus_are_well_formed_and_not_modelled() {
    // RLRQ [APPLICATION 2] and RLRE [APPLICATION 3]: "currently not used",
    // "included for completeness only" (3.2.1/Q.773).
    for pdu in ["62 00", "63 03 80 01 00"] {
        let portion = portion(DIALOGUE_AS, pdu);
        match portion.parse() {
            Ok(DialogueContent::Unmodelled(external)) => {
                assert_eq!(
                    external.encoding,
                    ExternalEncoding::SingleAsn1Type(Any::new(vector(pdu)))
                );
            }
            other => panic!("{pdu}: {other:?}"),
        }
        assert_eq!(portion.dialogue_pdu(), Ok(None));
    }
}

#[test]
fn an_external_without_a_direct_reference_is_not_modelled() {
    // indirect-reference 3, octet-aligned.
    let portion = DialoguePortion {
        external: Any::new(vector("28 07  02 01 03  81 02 01 02")),
    };
    match portion.parse() {
        Ok(DialogueContent::Unmodelled(external)) => {
            assert_eq!(external.direct_reference, None);
            assert_eq!(external.indirect_reference, Some(3));
            assert_eq!(
                external.encoding,
                ExternalEncoding::OctetAligned(vec![0x01, 0x02])
            );
        }
        other => panic!("{other:?}"),
    }
}

// ── Reading: malformed ──────────────────────────────────────────────────────

#[track_caller]
fn malformed(portion: DialoguePortion, what: &str) {
    match portion.parse() {
        Err(_) => {}
        Ok(read) => panic!("{what}: read as {read:?}"),
    }
    assert!(portion.dialogue_pdu().is_err(), "{what}");
    // And the decoder does not let a message with it through.
    let wire = {
        let external = portion.external.as_bytes();
        let mut wire = vec![0x62, (8 + external.len()) as u8];
        wire.extend_from_slice(&[0x48, 0x04, 0x00, 0x00, 0x10, 0x01]);
        wire.extend_from_slice(&[0x6b, external.len() as u8]);
        wire.extend_from_slice(external);
        wire
    };
    let found = common::problem(&wire);
    assert_eq!(found.fault, tcap::Fault::DialoguePortion, "{what}: {found}");
}

const AC: &str = "a1 09 06 07 04 00 00 01 00 1a 02";
const VERSION: &str = "80 02 07 80";

#[test]
fn a_malformed_aarq_is_malformed() {
    let cases = [
        ("no application-context-name", format!("60 04 {VERSION}")),
        (
            "application-context-name holding an INTEGER",
            format!("60 09 {VERSION} a1 03 02 01 05"),
        ),
        (
            "application-context-name in primitive form",
            format!("60 0d {VERSION} 81 07 04 00 00 01 00 1a 02"),
        ),
        (
            "application-context-name holding two values",
            "60 16 a1 14 06 07 04 00 00 01 00 1a 02 06 09 04 00 00 01 00 1a 02 00 00".to_string(),
        ),
        (
            "an object identifier that does not end",
            format!("60 09 {VERSION} a1 03 06 01 86"),
        ),
        ("members out of order", format!("60 0f {AC} {VERSION}")),
        (
            "a member an AARQ does not have",
            format!("60 14 {VERSION} {AC} a2 03 02 01 00"),
        ),
        (
            "protocol-version with no content",
            format!("60 0d 80 00 {AC}"),
        ),
        (
            "protocol-version with eight unused bits",
            format!("60 0f 80 02 08 80 {AC}"),
        ),
        (
            "protocol-version in constructed form",
            format!("60 11 a0 04 03 02 07 80 {AC}"),
        ),
        (
            "user-information holding something that is not an EXTERNAL",
            format!("60 14 {VERSION} {AC} be 03 04 01 01"),
        ),
        (
            "user-information with an EXTERNAL that has no encoding",
            format!("60 18 {VERSION} {AC} be 07 28 05 06 03 88 37 01"),
        ),
        (
            "user-information in primitive form",
            format!("60 12 {VERSION} {AC} 9e 01 00"),
        ),
        (
            "an element running past the end of the PDU",
            format!("60 0f {VERSION} a1 0b 06 07 04 00 00 01 00 1a 02"),
        ),
    ];
    for (what, pdu) in cases {
        malformed(portion(DIALOGUE_AS, &pdu), what);
        // The unstructured dialogue PDU has the same members.
        malformed(portion(UNIDIALOGUE_AS, &pdu), what);
    }
}

#[test]
fn a_malformed_aare_is_malformed() {
    let result = "a2 03 02 01 00";
    let diagnostic = "a3 05 a1 03 02 01 00";
    let cases = [
        ("no result", format!("61 16 {VERSION} {AC} {diagnostic}")),
        (
            "no result-source-diagnostic",
            format!("61 14 {VERSION} {AC} {result}"),
        ),
        (
            "result 2, which Q.773 does not define",
            format!("61 1b {VERSION} {AC} a2 03 02 01 02 {diagnostic}"),
        ),
        (
            "result as an implicit INTEGER",
            format!("61 19 {VERSION} {AC} 82 01 00 {diagnostic}"),
        ),
        (
            "diagnostic from a source [3]",
            format!("61 1b {VERSION} {AC} {result} a3 05 a3 03 02 01 00"),
        ),
        (
            "diagnostic holding two sources",
            format!("61 20 {VERSION} {AC} {result} a3 0a a1 03 02 01 00 a2 03 02 01 00"),
        ),
        (
            "diagnostic value that is not an INTEGER",
            format!("61 1b {VERSION} {AC} {result} a3 05 a1 03 04 01 00"),
        ),
        (
            "result and diagnostic swapped",
            format!("61 1b {VERSION} {AC} {diagnostic} {result}"),
        ),
        (
            "an INTEGER with no content",
            format!("61 1a {VERSION} {AC} a2 02 02 00 {diagnostic}"),
        ),
    ];
    for (what, pdu) in cases {
        malformed(portion(DIALOGUE_AS, &pdu), what);
    }
}

#[test]
fn a_malformed_abrt_is_malformed() {
    let cases = [
        ("no abort-source", "64 00"),
        (
            "abort-source 2, which Q.773 does not define",
            "64 03 80 01 02",
        ),
        ("abort-source as a universal INTEGER", "64 03 02 01 00"),
        ("abort-source in constructed form", "64 05 a0 03 02 01 00"),
        (
            "a member an ABRT does not have",
            "64 08 80 01 00 a2 03 02 01 00",
        ),
        ("abort-source twice", "64 06 80 01 00 80 01 01"),
    ];
    for (what, pdu) in cases {
        malformed(portion(DIALOGUE_AS, pdu), what);
    }
}

#[test]
fn a_malformed_external_is_malformed() {
    let cases = [
        ("a SEQUENCE", "30 0b 06 07 00 11 86 05 01 01 01 a0 00"),
        (
            "an EXTERNAL in primitive form",
            "08 10 06 07 00 11 86 05 01 01 01 a0 05 64 03 80 01 01",
        ),
        (
            "an EXTERNAL without an encoding",
            "28 09 06 07 00 11 86 05 01 01 01",
        ),
        (
            "an empty single-ASN1-type",
            "28 0b 06 07 00 11 86 05 01 01 01 a0 00",
        ),
        (
            "two values in the single-ASN1-type",
            "28 15 06 07 00 11 86 05 01 01 01 a0 0a 64 03 80 01 01 64 03 80 01 01",
        ),
        (
            "an element after the encoding",
            "28 13 06 07 00 11 86 05 01 01 01 a0 05 64 03 80 01 01 05 00 00",
        ),
        (
            "an encoding choice [3]",
            "28 10 06 07 00 11 86 05 01 01 01 a3 05 64 03 80 01 01",
        ),
        (
            "two EXTERNALs",
            "28 0a 06 03 88 37 01 a0 03 04 01 01 28 0a 06 03 88 37 01 a0 03 04 01 01",
        ),
        (
            "a PDU that is not one of the dialogue syntax",
            "28 0d 06 07 00 11 86 05 01 01 01 a0 02 65 00",
        ),
        (
            "an AARE under the unidialogue syntax",
            "28 0d 06 07 00 11 86 05 01 02 01 a0 02 61 00",
        ),
        (
            "a universal SEQUENCE where the PDU belongs",
            "28 0d 06 07 00 11 86 05 01 01 01 a0 02 30 00",
        ),
        (
            "a bit string that is not whole octets carrying the PDU",
            "28 11 06 07 00 11 86 05 01 01 01 82 06 03 64 03 80 01 00",
        ),
        ("nothing", ""),
    ];
    for (what, external) in cases {
        malformed(
            DialoguePortion {
                external: Any::new(vector(external)),
            },
            what,
        );
    }
}

// ── The values Q.773 names ──────────────────────────────────────────────────

#[test]
fn named_values_have_the_numbers_of_q773() {
    assert_eq!(AssociateResult::Accepted.value(), 0);
    assert_eq!(AssociateResult::RejectedPermanent.value(), 1);
    assert_eq!(AssociateResult::from_value(2), None);
    assert_eq!(AbortSource::DialogueServiceUser.value(), 0);
    assert_eq!(AbortSource::DialogueServiceProvider.value(), 1);
    assert_eq!(AbortSource::from_value(2), None);
    assert_eq!(
        AssociateSourceDiagnostic::USER_NULL,
        AssociateSourceDiagnostic::DialogueServiceUser(0)
    );
    assert_eq!(
        AssociateSourceDiagnostic::USER_NO_REASON_GIVEN,
        AssociateSourceDiagnostic::DialogueServiceUser(1)
    );
    assert_eq!(
        AssociateSourceDiagnostic::USER_APPLICATION_CONTEXT_NAME_NOT_SUPPORTED,
        AssociateSourceDiagnostic::DialogueServiceUser(2)
    );
    assert_eq!(
        AssociateSourceDiagnostic::PROVIDER_NULL,
        AssociateSourceDiagnostic::DialogueServiceProvider(0)
    );
    assert_eq!(
        AssociateSourceDiagnostic::PROVIDER_NO_REASON_GIVEN,
        AssociateSourceDiagnostic::DialogueServiceProvider(1)
    );
    assert_eq!(
        AssociateSourceDiagnostic::PROVIDER_NO_COMMON_DIALOGUE_PORTION,
        AssociateSourceDiagnostic::DialogueServiceProvider(2)
    );
    assert_eq!(tcap::dialogue::DIALOGUE_AS_OID, &[0, 0, 17, 773, 1, 1, 1]);
    assert_eq!(
        tcap::dialogue::UNIDIALOGUE_AS_OID,
        &[0, 0, 17, 773, 1, 2, 1]
    );
}
