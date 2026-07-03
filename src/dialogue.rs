//! Dialogue portion — the `EXTERNAL`-wrapped dialogue PDUs (AARQ / AARE / ABRT)
//! per ITU-T Q.773 and X.880, used for application-context negotiation.
//!
//! # Two layers
//!
//! [`DialoguePortion`] keeps its raw escape hatch — a single [`rasn::types::Any`]
//! holding the BER `EXTERNAL` value — so a caller can still carry any dialogue
//! bytes verbatim. On top of that sits a **typed** layer: the [`DialoguePdu`]
//! enum and small result/diagnostic enums, with builders that produce byte-exact
//! wire output and a [`DialoguePortion::dialogue_pdu`] parser that reads a
//! received dialogue portion back into the typed form.
//!
//! # Wire shape (BER)
//!
//! The parent transaction struct tags this field `[APPLICATION 11]` (implicit),
//! so `external` holds the **EXTERNAL** value *without* the outer `0x6B` — it
//! starts at the EXTERNAL tag `0x28`:
//!
//! ```text
//! EXTERNAL  [UNIVERSAL 8] constructed  (0x28)
//!   OBJECT IDENTIFIER  0.0.17.773.1.1.1   (id-as-dialogue, the dialogue-AS)
//!   [0] single-ASN1-type                 (0xA0)
//!       DialoguePDU
//! ```
//!
//! where `DialoguePDU` is one of:
//!
//! ```text
//! AARQ-apdu ::= [APPLICATION 0] IMPLICIT SEQUENCE {   (0x60)
//!     protocol-version         [0] IMPLICIT BIT STRING {version1(0)} DEFAULT,  (0x80 02 07 80)
//!     application-context-name [1] OBJECT IDENTIFIER,                          (0xA1 { 0x06 OID })
//!     user-information         [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }   (0xBE ...)
//!
//! AARE-apdu ::= [APPLICATION 1] IMPLICIT SEQUENCE {   (0x61)
//!     protocol-version         [0] IMPLICIT BIT STRING {version1(0)} DEFAULT,  (0x80 02 07 80)
//!     application-context-name [1] OBJECT IDENTIFIER,                          (0xA1 { 0x06 OID })
//!     result                   [2] Associate-result,                          (0xA2 { 0x02 len n })
//!     result-source-diagnostic [3] Associate-source-diagnostic,               (0xA3 { ... })
//!     user-information         [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }   (0xBE ...)
//!
//! ABRT-apdu ::= [APPLICATION 4] IMPLICIT SEQUENCE {   (0x64)
//!     abort-source     [0] IMPLICIT ABRT-source,                              (0x80 01 n)
//!     user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }          (0xBE ...)
//! ```
//!
//! `protocol-version` is a BIT STRING with only `version1(0)` set: under the
//! implicit `[0]` tag that is `80 02 07 80` (one "7 unused bits" octet, then the
//! `0x80` value octet). It is a DEFAULT, so builders always emit it and the
//! parser treats its absence as `Version1`.
//!
//! `user_information` stays **opaque bytes** (`Option<Vec<u8>>`): the raw content
//! octets of the `[30] IMPLICIT` `SEQUENCE OF EXTERNAL` — enough to carry a
//! nested APDU. It is not further modelled here. The structured-dialogue AUDT
//! variant (X.881) is out of scope.

use rasn::prelude::*;
use rasn::types::{Any, Oid};

/// Application Context Name — an OID identifying the MAP/CAP operation set.
pub type ApplicationContextName = rasn::types::ObjectIdentifier;

/// `id-as-dialogue` — the dialogue abstract syntax OID `0.0.17.773.1.1.1`
/// (`{itu-t recommendation q 773 as(1) dialogue-as(1) version1(1)}`), the
/// direct-reference carried in the `EXTERNAL` around every dialogue PDU.
pub const DIALOGUE_AS_OID: &[u32] = &[0, 0, 17, 773, 1, 1, 1];

/// Protocol version of a dialogue PDU. Only `version1` is defined by Q.773.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProtocolVersion {
    /// `version1(0)` — the only defined value; the DEFAULT.
    #[default]
    Version1,
}

/// `Associate-result` — the outcome carried in an AARE (X.880).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociateResult {
    /// `accepted(0)` — the dialogue is accepted.
    Accepted,
    /// `reject-permanent(1)` — rejected, do not retry.
    RejectedPermanent,
    /// `reject-transient(2)` — rejected, a retry may succeed.
    RejectedTransient,
}

impl AssociateResult {
    /// The INTEGER value on the wire.
    pub fn value(self) -> i64 {
        match self {
            Self::Accepted => 0,
            Self::RejectedPermanent => 1,
            Self::RejectedTransient => 2,
        }
    }

    fn from_value(v: i64) -> Option<Self> {
        match v {
            0 => Some(Self::Accepted),
            1 => Some(Self::RejectedPermanent),
            2 => Some(Self::RejectedTransient),
            _ => None,
        }
    }
}

/// `Associate-source-diagnostic` — why a dialogue got its result (X.880). The
/// inner integer is the diagnostic reason as defined for each source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociateSourceDiagnostic {
    /// `dialogue-service-user [1] INTEGER` — diagnostic from the ACSE user. `0`
    /// is `null` (the value paired with `accepted`).
    DialogueServiceUser(i64),
    /// `dialogue-service-provider [2] INTEGER` — diagnostic from the provider.
    DialogueServiceProvider(i64),
}

/// `ABRT-source` — who aborted the dialogue (X.880).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortSource {
    /// `dialogue-service-user(0)`.
    DialogueServiceUser,
    /// `dialogue-service-provider(1)`.
    DialogueServiceProvider,
}

impl AbortSource {
    /// The INTEGER value on the wire.
    pub fn value(self) -> i64 {
        match self {
            Self::DialogueServiceUser => 0,
            Self::DialogueServiceProvider => 1,
        }
    }

    fn from_value(v: i64) -> Option<Self> {
        match v {
            0 => Some(Self::DialogueServiceUser),
            1 => Some(Self::DialogueServiceProvider),
            _ => None,
        }
    }
}

/// A typed dialogue PDU (X.880), the payload inside the `EXTERNAL` of a
/// [`DialoguePortion`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialoguePdu {
    /// `AARQ-apdu` — the dialogue request, on the initiating leg (TCAP Begin).
    Aarq {
        /// Protocol version (DEFAULT `version1`).
        protocol_version: ProtocolVersion,
        /// The negotiated application context.
        application_context_name: ApplicationContextName,
        /// Optional user-information (opaque `[30]` content octets).
        user_information: Option<Vec<u8>>,
    },
    /// `AARE-apdu` — the dialogue response, on the accepting leg (TCAP End/Continue).
    Aare {
        /// Protocol version (DEFAULT `version1`).
        protocol_version: ProtocolVersion,
        /// The accepted application context.
        application_context_name: ApplicationContextName,
        /// The associate result.
        result: AssociateResult,
        /// The result source diagnostic.
        result_source_diagnostic: AssociateSourceDiagnostic,
        /// Optional user-information (opaque `[30]` content octets).
        user_information: Option<Vec<u8>>,
    },
    /// `ABRT-apdu` — an abort from the dialogue user/provider.
    Abrt {
        /// Who aborted.
        abort_source: AbortSource,
        /// Optional user-information (opaque `[30]` content octets).
        user_information: Option<Vec<u8>>,
    },
}

// ── BER helpers (definite length, matches ITU-T X.690) ───────────────────────

/// Encode a BER TLV with a definite length. Handles the long form for
/// completeness, though every dialogue portion is well under 127 octets.
fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + value.len());
    out.push(tag);
    if value.len() < 0x80 {
        out.push(value.len() as u8);
    } else {
        let mut len_bytes = Vec::new();
        let mut n = value.len();
        while n > 0 {
            len_bytes.insert(0, (n & 0xFF) as u8);
            n >>= 8;
        }
        out.push(0x80 | len_bytes.len() as u8);
        out.extend(len_bytes);
    }
    out.extend_from_slice(value);
    out
}

/// Encode an OID's arcs to BER OID content octets (no tag/length).
fn oid_contents(arcs: &[u32]) -> Vec<u8> {
    let mut body = vec![(40 * arcs[0] + arcs[1]) as u8];
    for &arc in &arcs[2..] {
        let mut a = arc;
        if a == 0 {
            body.push(0);
            continue;
        }
        let mut chunk = Vec::new();
        while a > 0 {
            chunk.insert(0, (a & 0x7F) as u8);
            a >>= 7;
        }
        let last = chunk.len() - 1;
        for b in chunk.iter_mut().take(last) {
            *b |= 0x80;
        }
        body.extend(chunk);
    }
    body
}

/// Encode an INTEGER's content octets (minimal two's-complement, big-endian).
/// Dialogue integers here are small and non-negative, but negatives stay correct.
fn int_contents(v: i64) -> Vec<u8> {
    if v == 0 {
        return vec![0x00];
    }
    let mut bytes = v.to_be_bytes().to_vec();
    // Drop redundant leading sign-extension octets while the next octet keeps
    // the sign bit intact (0x00 before a clear high bit, 0xFF before a set one).
    while bytes.len() > 1 {
        let redundant = (bytes[0] == 0x00 && bytes[1] & 0x80 == 0)
            || (bytes[0] == 0xFF && bytes[1] & 0x80 != 0);
        if redundant {
            bytes.remove(0);
        } else {
            break;
        }
    }
    bytes
}

/// `protocol-version [0] IMPLICIT BIT STRING { version1(0) }` = `80 02 07 80`.
fn protocol_version_tlv() -> Vec<u8> {
    tlv(0x80, &[0x07, 0x80])
}

/// `application-context-name [1] { OBJECT IDENTIFIER }`.
fn application_context_name_tlv(ac: &[u32]) -> Vec<u8> {
    tlv(0xA1, &tlv(0x06, &oid_contents(ac)))
}

/// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL` from opaque content.
fn user_information_tlv(ui: &[u8]) -> Vec<u8> {
    tlv(0xBE, ui)
}

/// Wrap a DialoguePDU in the `EXTERNAL` and build a [`DialoguePortion`].
fn external(dialogue_pdu: &[u8]) -> DialoguePortion {
    let content = [
        tlv(0x06, &oid_contents(DIALOGUE_AS_OID)), // direct-reference OID
        tlv(0xA0, dialogue_pdu),                   // [0] single-ASN1-type
    ]
    .concat();
    // EXTERNAL is [UNIVERSAL 8] constructed = 0x28. The parent transaction adds
    // the outer [APPLICATION 11] (0x6B) implicit tag around this Any.
    DialoguePortion {
        external: Any::new(tlv(0x28, &content)),
    }
}

// ── Minimal BER reader for the parser ────────────────────────────────────────

/// Parse one BER TLV at the start of `input`. Returns `(tag, value, rest)`.
/// Supports the definite short and long length forms.
fn read_tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *input.first()?;
    let first_len = *input.get(1)?;
    let (len, header) = if first_len < 0x80 {
        (first_len as usize, 2)
    } else {
        let n = (first_len & 0x7F) as usize;
        if n == 0 || n > 4 {
            return None; // indefinite or absurd — not used by dialogue portions
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | (*input.get(2 + i)? as usize);
        }
        (len, 2 + n)
    };
    let value = input.get(header..header + len)?;
    let rest = &input[header + len..];
    Some((tag, value, rest))
}

/// Parse an OID's content octets back into arcs.
fn parse_oid(contents: &[u8]) -> Option<Vec<u32>> {
    let first = *contents.first()?;
    let mut arcs = vec![(first / 40) as u32, (first % 40) as u32];
    let mut value = 0u32;
    let mut pending = false;
    for &b in &contents[1..] {
        value = value.checked_mul(128)?.checked_add((b & 0x7F) as u32)?;
        pending = true;
        if b & 0x80 == 0 {
            arcs.push(value);
            value = 0;
            pending = false;
        }
    }
    if pending {
        return None; // truncated multi-byte arc
    }
    Some(arcs)
}

/// Parse an INTEGER's content octets (as used here: small, non-negative).
fn parse_int(contents: &[u8]) -> Option<i64> {
    if contents.is_empty() || contents.len() > 8 {
        return None;
    }
    let mut v: i64 = if contents[0] & 0x80 != 0 { -1 } else { 0 };
    for &b in contents {
        v = (v << 8) | (b as i64);
    }
    Some(v)
}

/// Dialogue Portion — carries the `EXTERNAL` dialogue PDU (AARQ / AARE / ABRT)
/// per ITU-T Q.773.
///
/// With the parent's implicit `[APPLICATION 11]` tag, rasn produces
/// `6B len { EXTERNAL_bytes }`, which is wire-correct. The [`external`](Self::external)
/// field holds the BER `EXTERNAL` content starting at the EXTERNAL tag `0x28`.
///
/// Use the typed builders ([`aarq`](Self::aarq), [`aare_accept`](Self::aare_accept),
/// [`abrt`](Self::abrt), [`from_pdu`](Self::from_pdu)) to construct one and
/// [`dialogue_pdu`](Self::dialogue_pdu) to read one back.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct DialoguePortion {
    /// The `EXTERNAL` type containing the dialogue PDU (AARQ / AARE / ABRT).
    pub external: rasn::types::Any,
}

impl DialoguePortion {
    /// Build a dialogue portion from a typed [`DialoguePdu`].
    pub fn from_pdu(pdu: &DialoguePdu) -> Self {
        let bytes = match pdu {
            DialoguePdu::Aarq {
                application_context_name,
                user_information,
                ..
            } => {
                let mut body = protocol_version_tlv();
                body.extend(application_context_name_tlv(
                    application_context_name.as_ref(),
                ));
                if let Some(ui) = user_information {
                    body.extend(user_information_tlv(ui));
                }
                tlv(0x60, &body)
            }
            DialoguePdu::Aare {
                application_context_name,
                result,
                result_source_diagnostic,
                user_information,
                ..
            } => {
                let mut body = protocol_version_tlv();
                body.extend(application_context_name_tlv(
                    application_context_name.as_ref(),
                ));
                body.extend(tlv(0xA2, &tlv(0x02, &int_contents(result.value()))));
                body.extend(source_diagnostic_tlv(*result_source_diagnostic));
                if let Some(ui) = user_information {
                    body.extend(user_information_tlv(ui));
                }
                tlv(0x61, &body)
            }
            DialoguePdu::Abrt {
                abort_source,
                user_information,
            } => {
                let mut body = tlv(0x80, &int_contents(abort_source.value()));
                if let Some(ui) = user_information {
                    body.extend(user_information_tlv(ui));
                }
                tlv(0x64, &body)
            }
        };
        external(&bytes)
    }

    /// Build an **AARQ** dialogue portion carrying `ac` as the application
    /// context (protocol version `version1`, no user-information). Used on the
    /// initiating leg (TCAP Begin).
    pub fn aarq(ac: &Oid) -> Self {
        Self::from_pdu(&DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            user_information: None,
        })
    }

    /// Build an accepting **AARE** dialogue portion carrying `ac`, result
    /// `accepted(0)`, and result-source-diagnostic `dialogue-service-user`
    /// `null(0)`. Used on the responding leg (TCAP End / Continue).
    pub fn aare_accept(ac: &Oid) -> Self {
        Self::from_pdu(&DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            result: AssociateResult::Accepted,
            result_source_diagnostic: AssociateSourceDiagnostic::DialogueServiceUser(0),
            user_information: None,
        })
    }

    /// Build an **ABRT** dialogue portion with the given abort source and no
    /// user-information.
    pub fn abrt(source: AbortSource) -> Self {
        Self::from_pdu(&DialoguePdu::Abrt {
            abort_source: source,
            user_information: None,
        })
    }

    /// Parse the `external` bytes back into a typed [`DialoguePdu`], or `None`
    /// if they are not a well-formed AARQ / AARE / ABRT (e.g. an opaque or
    /// structured-dialogue portion this typed layer does not model).
    pub fn dialogue_pdu(&self) -> Option<DialoguePdu> {
        let bytes = self.external.as_bytes();
        // EXTERNAL [UNIVERSAL 8] constructed.
        let (tag, ext, _) = read_tlv(bytes)?;
        if tag != 0x28 {
            return None;
        }
        // direct-reference OID, then [0] single-ASN1-type.
        let (oid_tag, oid_val, rest) = read_tlv(ext)?;
        if oid_tag != 0x06 || parse_oid(oid_val)? != DIALOGUE_AS_OID {
            return None;
        }
        let (a0_tag, pdu, _) = read_tlv(rest)?;
        if a0_tag != 0xA0 {
            return None;
        }
        // The DialoguePDU itself.
        let (pdu_tag, body, _) = read_tlv(pdu)?;
        match pdu_tag {
            0x60 => parse_aarq(body),
            0x61 => parse_aare(body),
            0x64 => parse_abrt(body),
            _ => None,
        }
    }
}

/// `result-source-diagnostic [3] { dialogue-service-user/provider [n] INTEGER }`.
fn source_diagnostic_tlv(diag: AssociateSourceDiagnostic) -> Vec<u8> {
    let inner = match diag {
        AssociateSourceDiagnostic::DialogueServiceUser(v) => {
            tlv(0xA1, &tlv(0x02, &int_contents(v)))
        }
        AssociateSourceDiagnostic::DialogueServiceProvider(v) => {
            tlv(0xA2, &tlv(0x02, &int_contents(v)))
        }
    };
    tlv(0xA3, &inner)
}

/// Read the DEFAULTed protocol-version off the front of a PDU body, returning
/// the version and the remaining body. Absence is treated as `version1`.
fn take_protocol_version(body: &[u8]) -> (ProtocolVersion, &[u8]) {
    if let Some((0x80, _, rest)) = read_tlv(body) {
        (ProtocolVersion::Version1, rest)
    } else {
        (ProtocolVersion::Version1, body)
    }
}

/// Read the OPTIONAL trailing `user-information [30]` off a PDU body.
fn take_user_information(body: &[u8]) -> Option<Vec<u8>> {
    let (tag, value, _) = read_tlv(body)?;
    (tag == 0xBE).then(|| value.to_vec())
}

fn parse_aarq(body: &[u8]) -> Option<DialoguePdu> {
    let (protocol_version, rest) = take_protocol_version(body);
    let (ac_tag, ac_wrap, rest) = read_tlv(rest)?;
    if ac_tag != 0xA1 {
        return None;
    }
    let (oid_tag, oid_val, _) = read_tlv(ac_wrap)?;
    if oid_tag != 0x06 {
        return None;
    }
    let application_context_name = ApplicationContextName::new(parse_oid(oid_val)?)?;
    let user_information = take_user_information(rest);
    Some(DialoguePdu::Aarq {
        protocol_version,
        application_context_name,
        user_information,
    })
}

fn parse_aare(body: &[u8]) -> Option<DialoguePdu> {
    let (protocol_version, rest) = take_protocol_version(body);
    let (ac_tag, ac_wrap, rest) = read_tlv(rest)?;
    if ac_tag != 0xA1 {
        return None;
    }
    let (oid_tag, oid_val, _) = read_tlv(ac_wrap)?;
    if oid_tag != 0x06 {
        return None;
    }
    let application_context_name = ApplicationContextName::new(parse_oid(oid_val)?)?;

    let (res_tag, res_wrap, rest) = read_tlv(rest)?;
    if res_tag != 0xA2 {
        return None;
    }
    let (int_tag, int_val, _) = read_tlv(res_wrap)?;
    if int_tag != 0x02 {
        return None;
    }
    let result = AssociateResult::from_value(parse_int(int_val)?)?;

    let (diag_tag, diag_wrap, rest) = read_tlv(rest)?;
    if diag_tag != 0xA3 {
        return None;
    }
    let (src_tag, src_wrap, _) = read_tlv(diag_wrap)?;
    let (v_tag, v_val, _) = read_tlv(src_wrap)?;
    if v_tag != 0x02 {
        return None;
    }
    let v = parse_int(v_val)?;
    let result_source_diagnostic = match src_tag {
        0xA1 => AssociateSourceDiagnostic::DialogueServiceUser(v),
        0xA2 => AssociateSourceDiagnostic::DialogueServiceProvider(v),
        _ => return None,
    };

    let user_information = take_user_information(rest);
    Some(DialoguePdu::Aare {
        protocol_version,
        application_context_name,
        result,
        result_source_diagnostic,
        user_information,
    })
}

fn parse_abrt(body: &[u8]) -> Option<DialoguePdu> {
    let (src_tag, src_val, rest) = read_tlv(body)?;
    if src_tag != 0x80 {
        return None;
    }
    let abort_source = AbortSource::from_value(parse_int(src_val)?)?;
    let user_information = take_user_information(rest);
    Some(DialoguePdu::Abrt {
        abort_source,
        user_information,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // MAP shortMsgGateway v3 — SRI-SM application context (0.4.0.0.1.0.20.3).
    const MAP_SRI_SM_AC: &[u32] = &[0, 4, 0, 0, 1, 0, 20, 3];
    // CAP gsmSSF-scfGeneric v3 — CAMEL call application context (0.4.0.0.1.21.3.4).
    const CAP_GSMSSF_SCF_AC: &[u32] = &[0, 4, 0, 0, 1, 21, 3, 4];

    fn oid(arcs: &[u32]) -> &Oid {
        Oid::new(arcs).unwrap()
    }

    #[test]
    fn oid_contents_short_msg_gateway() {
        // 0.4.0.0.1.0.20.3 → 04 00 00 01 00 14 03 (0*40+4 = 0x04).
        assert_eq!(
            oid_contents(MAP_SRI_SM_AC),
            vec![0x04, 0x00, 0x00, 0x01, 0x00, 0x14, 0x03]
        );
    }

    #[test]
    fn oid_contents_dialogue_as() {
        // 0.0.17.773.1.1.1 → 00 11 86 05 01 01 01 (773 = 0x86 0x05 multibyte).
        assert_eq!(
            oid_contents(DIALOGUE_AS_OID),
            vec![0x00, 0x11, 0x86, 0x05, 0x01, 0x01, 0x01]
        );
    }

    #[test]
    fn oid_round_trips_through_parser() {
        for arcs in [MAP_SRI_SM_AC, CAP_GSMSSF_SCF_AC, DIALOGUE_AS_OID] {
            assert_eq!(parse_oid(&oid_contents(arcs)).unwrap(), arcs);
        }
    }

    /// Byte-exactness: the AARQ bytes must equal the proven ss7-stack layout —
    /// EXTERNAL(0x28) → OID(dialogue-as) → [0] → AARQ(0x60) → protocol-version →
    /// application-context-name.
    #[test]
    fn aarq_bytes_are_byte_exact() {
        let dp = DialoguePortion::aarq(oid(MAP_SRI_SM_AC));
        let ac = oid_contents(MAP_SRI_SM_AC);
        // AARQ body: protocol-version (80 02 07 80) + AC (A1 { 06 OID }).
        let mut aarq_body = vec![0x80, 0x02, 0x07, 0x80];
        aarq_body.extend([0xA1, (2 + ac.len()) as u8, 0x06, ac.len() as u8]);
        aarq_body.extend(&ac);
        let aarq = {
            let mut v = vec![0x60, aarq_body.len() as u8];
            v.extend(&aarq_body);
            v
        };
        let das = oid_contents(DIALOGUE_AS_OID);
        let mut ext_content = vec![0x06, das.len() as u8];
        ext_content.extend(&das);
        ext_content.extend([0xA0, aarq.len() as u8]);
        ext_content.extend(&aarq);
        let mut expected = vec![0x28, ext_content.len() as u8];
        expected.extend(&ext_content);
        assert_eq!(dp.external.as_bytes(), expected.as_slice());
        // Must start at the EXTERNAL tag, not the outer [APPLICATION 11] (0x6B).
        assert_eq!(dp.external.as_bytes()[0], 0x28);
    }

    #[test]
    fn aare_bytes_are_byte_exact() {
        let dp = DialoguePortion::aare_accept(oid(CAP_GSMSSF_SCF_AC));
        let ac = oid_contents(CAP_GSMSSF_SCF_AC);
        let mut aare_body = vec![0x80, 0x02, 0x07, 0x80];
        aare_body.extend([0xA1, (2 + ac.len()) as u8, 0x06, ac.len() as u8]);
        aare_body.extend(&ac);
        aare_body.extend([0xA2, 0x03, 0x02, 0x01, 0x00]); // result: accepted(0)
        aare_body.extend([0xA3, 0x05, 0xA1, 0x03, 0x02, 0x01, 0x00]); // diag: user null(0)
        let aare = {
            let mut v = vec![0x61, aare_body.len() as u8];
            v.extend(&aare_body);
            v
        };
        let das = oid_contents(DIALOGUE_AS_OID);
        let mut ext_content = vec![0x06, das.len() as u8];
        ext_content.extend(&das);
        ext_content.extend([0xA0, aare.len() as u8]);
        ext_content.extend(&aare);
        let mut expected = vec![0x28, ext_content.len() as u8];
        expected.extend(&ext_content);
        assert_eq!(dp.external.as_bytes(), expected.as_slice());
    }

    #[test]
    fn aarq_round_trip() {
        let pdu = DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: oid(MAP_SRI_SM_AC).into(),
            user_information: None,
        };
        let dp = DialoguePortion::from_pdu(&pdu);
        assert_eq!(dp.dialogue_pdu(), Some(pdu));
    }

    #[test]
    fn aare_round_trip() {
        let pdu = DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: oid(CAP_GSMSSF_SCF_AC).into(),
            result: AssociateResult::Accepted,
            result_source_diagnostic: AssociateSourceDiagnostic::DialogueServiceUser(0),
            user_information: None,
        };
        let dp = DialoguePortion::from_pdu(&pdu);
        assert_eq!(dp.dialogue_pdu(), Some(pdu));
    }

    #[test]
    fn abrt_round_trip() {
        for src in [
            AbortSource::DialogueServiceUser,
            AbortSource::DialogueServiceProvider,
        ] {
            let pdu = DialoguePdu::Abrt {
                abort_source: src,
                user_information: None,
            };
            let dp = DialoguePortion::from_pdu(&pdu);
            assert_eq!(dp.dialogue_pdu(), Some(pdu));
        }
    }

    #[test]
    fn rejected_and_provider_diagnostic_round_trip() {
        let pdu = DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: oid(MAP_SRI_SM_AC).into(),
            result: AssociateResult::RejectedPermanent,
            result_source_diagnostic: AssociateSourceDiagnostic::DialogueServiceProvider(1),
            user_information: None,
        };
        let dp = DialoguePortion::from_pdu(&pdu);
        assert_eq!(dp.dialogue_pdu(), Some(pdu));
    }

    #[test]
    fn user_information_round_trip() {
        // Opaque [30] content: a single (fake) EXTERNAL of a few octets.
        let ui = vec![0x28, 0x03, 0x06, 0x01, 0x2A];
        let pdu = DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: oid(MAP_SRI_SM_AC).into(),
            user_information: Some(ui),
        };
        let dp = DialoguePortion::from_pdu(&pdu);
        assert_eq!(dp.dialogue_pdu(), Some(pdu));
    }

    #[test]
    fn builders_match_from_pdu() {
        assert_eq!(
            DialoguePortion::aarq(oid(MAP_SRI_SM_AC)),
            DialoguePortion::from_pdu(&DialoguePdu::Aarq {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(MAP_SRI_SM_AC).into(),
                user_information: None,
            })
        );
        assert_eq!(
            DialoguePortion::abrt(AbortSource::DialogueServiceProvider),
            DialoguePortion::from_pdu(&DialoguePdu::Abrt {
                abort_source: AbortSource::DialogueServiceProvider,
                user_information: None,
            })
        );
    }

    #[test]
    fn opaque_portion_parses_as_none() {
        // Not a dialogue-as EXTERNAL — the parser must return None, not panic.
        let dp = DialoguePortion {
            external: Any::new(vec![0x28, 0x03, 0x06, 0x01, 0x2A]),
        };
        assert_eq!(dp.dialogue_pdu(), None);
    }

    #[test]
    fn parsed_ac_matches_input() {
        let dp = DialoguePortion::aare_accept(oid(MAP_SRI_SM_AC));
        match dp.dialogue_pdu().unwrap() {
            DialoguePdu::Aare {
                application_context_name,
                result,
                ..
            } => {
                assert_eq!(application_context_name.as_ref(), MAP_SRI_SM_AC);
                assert_eq!(result, AssociateResult::Accepted);
            }
            _ => panic!("expected AARE"),
        }
    }
}
