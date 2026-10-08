//! Dialogue portion: the `EXTERNAL` carrying a dialogue control PDU (AARQ,
//! AARE, ABRT, AUDT) or user information, per ITU-T Q.773 clauses 3.2 and
//! 4.2.3.
//!
//! # Two layers
//!
//! [`DialoguePortion`] holds the BER of the `EXTERNAL` value as it travels,
//! so any dialogue portion can be carried verbatim. On top of that sits a
//! typed layer: [`DialoguePdu`] with builders that produce the Q.773 encoding
//! and [`DialoguePortion::parse`], which reads a received portion.
//!
//! # Three outcomes, never confused
//!
//! Reading a dialogue portion has three distinct results:
//!
//! * `Ok(DialogueContent::Pdu(..))`: a well-formed AARQ, AARE, ABRT or AUDT.
//! * `Ok(DialogueContent::Unmodelled(..))`: a well-formed `EXTERNAL` that
//!   carries something this crate does not model: user information in a
//!   user-defined abstract syntax (the Note to `Abort` in 3.1/Q.773 allows it),
//!   or an RLRQ / RLRE PDU, which Q.773 defines "for completeness only".
//! * `Err(DialogueError)`: the portion is there and is malformed. 3.2.2.1/Q.774
//!   has the component sub-layer abort the dialogue for a "syntactically
//!   incorrect" dialogue portion; see [`DialoguePortion::abnormal_dialogue`].
//!
//! An absent dialogue portion is the `None` of the message's
//! `dialogue_portion` member.
//!
//! # Wire shape
//!
//! `DialoguePortion ::= [APPLICATION 11] EXTERNAL` is an explicit tag, so the
//! message carries `6B len` around the `EXTERNAL` (`28 len ..`), and
//! [`DialoguePortion::external`] starts at the `28`:
//!
//! ```text
//! EXTERNAL  [UNIVERSAL 8] constructed            28
//!   direct-reference OBJECT IDENTIFIER           06   dialogue-as or unidialogue-as
//!   single-ASN1-type [0]                         A0
//!     DialoguePDU
//!
//! AARQ-apdu ::= [APPLICATION 0] IMPLICIT SEQUENCE {                    60
//!     protocol-version         [0] IMPLICIT BIT STRING { version1 (0) }
//!                                  DEFAULT { version1 },               80 02 07 80
//!     application-context-name [1] OBJECT IDENTIFIER,                  A1 { 06 .. }
//!     user-information         [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }   BE { 28 .. }
//!
//! AARE-apdu ::= [APPLICATION 1] IMPLICIT SEQUENCE {                    61
//!     protocol-version         [0] IMPLICIT BIT STRING ..,             80 02 07 80
//!     application-context-name [1] OBJECT IDENTIFIER,                  A1 { 06 .. }
//!     result                   [2] Associate-result,                   A2 { 02 01 n }
//!     result-source-diagnostic [3] Associate-source-diagnostic,        A3 { A1|A2 { 02 01 n } }
//!     user-information         [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }
//!
//! ABRT-apdu ::= [APPLICATION 4] IMPLICIT SEQUENCE {                    64
//!     abort-source     [0] IMPLICIT ABRT-source,                       80 01 n
//!     user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL }
//!
//! AUDT-apdu ::= [APPLICATION 0] IMPLICIT SEQUENCE {                    60
//!     protocol-version, application-context-name, user-information as in AARQ }
//! ```
//!
//! The modules of clause 3.2/Q.773 have no `IMPLICIT TAGS` default, so `[1]`,
//! `[2]` and `[3]` are explicit (constructed, wrapping the universal type) and
//! the members written `IMPLICIT` replace the universal tag.

use std::fmt;

use rasn::prelude::*;
use rasn::types::{Any, ObjectIdentifier, Oid};

use crate::ber::{self, Element, APPLICATION, CONTEXT, UNIVERSAL};

/// Application Context Name — an OID identifying the MAP/CAP operation set.
pub type ApplicationContextName = ObjectIdentifier;

/// `dialogue-as-id`: `{ ccitt recommendation q 773 as (1) dialogue-as (1)
/// version1 (1) }`, the abstract syntax of the structured dialogue PDUs
/// (AARQ, AARE, ABRT).
pub const DIALOGUE_AS_OID: &[u32] = &[0, 0, 17, 773, 1, 1, 1];

/// `uniDialogue-as-id`: `{ ccitt recommendation q 773 as (1) unidialogue-as
/// (2) version1 (1) }`, the abstract syntax of the unstructured dialogue PDU
/// (AUDT), carried in a Unidirectional message.
pub const UNIDIALOGUE_AS_OID: &[u32] = &[0, 0, 17, 773, 1, 2, 1];

const DIALOGUE_AS: &Oid = Oid::const_new(DIALOGUE_AS_OID);
const UNIDIALOGUE_AS: &Oid = Oid::const_new(UNIDIALOGUE_AS_OID);

/// A dialogue portion that is present and malformed.
///
/// 3.2.2.1/Q.774: the component sub-layer aborts the dialogue "when an
/// incorrect dialogue portion is received, i.e. syntactically incorrect"; the
/// local TC-user gets a TC-P-ABORT with "abnormal dialogue" and the peer an
/// ABRT APDU, see [`DialoguePortion::abnormal_dialogue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueError {
    detail: String,
}

impl DialogueError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// What is wrong, for logs.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DialogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed dialogue portion: {}", self.detail)
    }
}

impl std::error::Error for DialogueError {}

type Parsed<T> = Result<T, DialogueError>;

/// `protocol-version [0] IMPLICIT BIT STRING { version1 (0) }`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProtocolVersion {
    /// Bit `version1 (0)` is set. This is the DEFAULT, the only version Q.773
    /// defines, and what the builders emit (`80 02 07 80`).
    #[default]
    Version1,
    /// `version1` is not among the versions listed. Holds the BIT STRING
    /// content octets as received (the count of unused bits, then the bits).
    ///
    /// 3.2.3/Q.774: for such an AARQ the component sub-layer answers with
    /// [`DialoguePortion::no_common_dialogue_portion`] in an Abort; in an AARE
    /// it "shall be considered a syntax error", so the parser never returns
    /// this variant for an AARE.
    Other(Vec<u8>),
}

impl ProtocolVersion {
    fn content(&self) -> Vec<u8> {
        match self {
            // One octet of bits, seven of them unused, bit 0 (the most
            // significant) set.
            Self::Version1 => vec![0x07, 0x80],
            Self::Other(content) => content.clone(),
        }
    }

    fn from_content(content: &[u8]) -> Parsed<Self> {
        let (&unused, bits) = content
            .split_first()
            .ok_or_else(|| DialogueError::new("protocol-version BIT STRING has no content"))?;
        if unused > 7 || (bits.is_empty() && unused != 0) {
            return Err(DialogueError::new(
                "protocol-version BIT STRING has an invalid count of unused bits",
            ));
        }
        match bits.first() {
            Some(first) if first & 0x80 != 0 => Ok(Self::Version1),
            _ => Ok(Self::Other(content.to_vec())),
        }
    }
}

/// `Associate-result ::= INTEGER { accepted (0), reject-permanent (1) }`.
///
/// Q.773 defines these two values only. (ACSE in X.227 has a third,
/// reject-transient, which the TC dialogue PDUs did not take over.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociateResult {
    /// `accepted (0)`.
    Accepted,
    /// `reject-permanent (1)`.
    RejectedPermanent,
}

impl AssociateResult {
    /// The INTEGER value on the wire.
    pub fn value(self) -> i64 {
        match self {
            Self::Accepted => 0,
            Self::RejectedPermanent => 1,
        }
    }

    /// The result for a wire value, if Q.773 defines it.
    pub fn from_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Accepted),
            1 => Some(Self::RejectedPermanent),
            _ => None,
        }
    }
}

/// `Associate-source-diagnostic`: who produced the result of an AARE, and a
/// reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociateSourceDiagnostic {
    /// `dialogue-service-user [1] INTEGER { null (0), no-reason-given (1),
    /// application-context-name-not-supported (2) }`.
    DialogueServiceUser(i64),
    /// `dialogue-service-provider [2] INTEGER { null (0), no-reason-given (1),
    /// no-common-dialogue-portion (2) }`.
    DialogueServiceProvider(i64),
}

impl AssociateSourceDiagnostic {
    /// `dialogue-service-user: null (0)`, the value paired with `accepted`.
    pub const USER_NULL: Self = Self::DialogueServiceUser(0);
    /// `dialogue-service-user: no-reason-given (1)`.
    pub const USER_NO_REASON_GIVEN: Self = Self::DialogueServiceUser(1);
    /// `dialogue-service-user: application-context-name-not-supported (2)`.
    pub const USER_APPLICATION_CONTEXT_NAME_NOT_SUPPORTED: Self = Self::DialogueServiceUser(2);
    /// `dialogue-service-provider: null (0)`.
    pub const PROVIDER_NULL: Self = Self::DialogueServiceProvider(0);
    /// `dialogue-service-provider: no-reason-given (1)`.
    pub const PROVIDER_NO_REASON_GIVEN: Self = Self::DialogueServiceProvider(1);
    /// `dialogue-service-provider: no-common-dialogue-portion (2)`.
    pub const PROVIDER_NO_COMMON_DIALOGUE_PORTION: Self = Self::DialogueServiceProvider(2);
}

/// `ABRT-source ::= INTEGER { dialogue-service-user (0),
/// dialogue-service-provider (1) }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortSource {
    /// `dialogue-service-user (0)`.
    DialogueServiceUser,
    /// `dialogue-service-provider (1)`.
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

    /// The source for a wire value, if Q.773 defines it.
    pub fn from_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::DialogueServiceUser),
            1 => Some(Self::DialogueServiceProvider),
            _ => None,
        }
    }
}

/// How the data value of an [`External`] is carried (X.690 8.18.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalEncoding {
    /// `single-ASN1-type [0]`: the complete encoding (tag, length, content) of
    /// one ASN.1 value. This is the form Q.773 shows and the builders emit.
    SingleAsn1Type(Any),
    /// `octet-aligned [1] IMPLICIT OCTET STRING`: the content octets.
    OctetAligned(Vec<u8>),
    /// `arbitrary [2] IMPLICIT BIT STRING`: the count of unused bits in the
    /// last octet, and the octets.
    Arbitrary { unused_bits: u8, bits: Vec<u8> },
}

/// The ASN.1 `EXTERNAL` type as BER encodes it (X.690 8.18.1):
///
/// ```text
/// [UNIVERSAL 8] IMPLICIT SEQUENCE {
///     direct-reference      OBJECT IDENTIFIER OPTIONAL,
///     indirect-reference    INTEGER OPTIONAL,
///     data-value-descriptor ObjectDescriptor OPTIONAL,
///     encoding CHOICE {
///         single-ASN1-type [0] ABSTRACT-SYNTAX.&Type,
///         octet-aligned    [1] IMPLICIT OCTET STRING,
///         arbitrary        [2] IMPLICIT BIT STRING } }
/// ```
///
/// A dialogue portion is one `EXTERNAL`; the user information of a dialogue
/// PDU is a `SEQUENCE OF EXTERNAL`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct External {
    /// The abstract syntax of the data value.
    pub direct_reference: Option<ObjectIdentifier>,
    /// A presentation context identifier. Not used by TC.
    pub indirect_reference: Option<i64>,
    /// The content octets of the ObjectDescriptor. Not used by TC.
    pub data_value_descriptor: Option<Vec<u8>>,
    /// The data value.
    pub encoding: ExternalEncoding,
}

impl External {
    /// An `EXTERNAL` naming `abstract_syntax` and carrying `value`, the
    /// complete BER encoding of one ASN.1 value, as a single-ASN1-type.
    pub fn single_asn1_type(abstract_syntax: &Oid, value: Any) -> Self {
        Self {
            direct_reference: Some(abstract_syntax.into()),
            indirect_reference: None,
            data_value_descriptor: None,
            encoding: ExternalEncoding::SingleAsn1Type(value),
        }
    }

    /// The BER encoding, starting at the `EXTERNAL` tag `28`.
    pub fn encode(&self) -> Vec<u8> {
        let mut content = Vec::new();
        if let Some(reference) = &self.direct_reference {
            content.extend(ber::tlv(0x06, &oid_content(reference)));
        }
        if let Some(reference) = self.indirect_reference {
            content.extend(ber::tlv(0x02, &ber::integer_content(reference)));
        }
        if let Some(descriptor) = &self.data_value_descriptor {
            content.extend(ber::tlv(0x07, descriptor));
        }
        match &self.encoding {
            ExternalEncoding::SingleAsn1Type(value) => {
                content.extend(ber::tlv(0xa0, value.as_bytes()));
            }
            ExternalEncoding::OctetAligned(octets) => content.extend(ber::tlv(0x81, octets)),
            ExternalEncoding::Arbitrary { unused_bits, bits } => {
                let mut value = Vec::with_capacity(bits.len() + 1);
                value.push(*unused_bits);
                value.extend_from_slice(bits);
                content.extend(ber::tlv(0x82, &value));
            }
        }
        ber::tlv(0x28, &content)
    }

    /// Read one `EXTERNAL` from `bytes`, which must hold exactly that.
    pub fn decode(bytes: &[u8]) -> Result<Self, DialogueError> {
        let external = sole_element(bytes, "EXTERNAL")?;
        Self::from_element(&external)
    }

    fn from_element(external: &Element<'_>) -> Parsed<Self> {
        if !external.is(UNIVERSAL, 8) || !external.constructed {
            return Err(DialogueError::new(format!(
                "expected a constructed EXTERNAL [UNIVERSAL 8], found {}",
                external.tag()
            )));
        }
        let mut members = children(external.content, "EXTERNAL")?
            .into_iter()
            .peekable();

        let direct_reference = match members.next_if(|m| m.is(UNIVERSAL, 6)) {
            Some(member) => Some(object_identifier(&member, "direct-reference")?),
            None => None,
        };
        let indirect_reference = match members.next_if(|m| m.is(UNIVERSAL, 2)) {
            Some(member) => Some(integer(&member, "indirect-reference")?),
            None => None,
        };
        let data_value_descriptor = match members.next_if(|m| m.is(UNIVERSAL, 7)) {
            Some(member) => Some(primitive(&member, "data-value-descriptor")?.to_vec()),
            None => None,
        };
        let member = members
            .next()
            .ok_or_else(|| DialogueError::new("EXTERNAL has no encoding member"))?;
        let encoding = if member.is(CONTEXT, 0) {
            if !member.constructed {
                return Err(DialogueError::new(
                    "single-ASN1-type [0] is primitive, it has to be constructed",
                ));
            }
            let value = sole_element(member.content, "single-ASN1-type [0]")?;
            ExternalEncoding::SingleAsn1Type(Any::new(value.whole.to_vec()))
        } else if member.is(CONTEXT, 1) {
            ExternalEncoding::OctetAligned(primitive(&member, "octet-aligned [1]")?.to_vec())
        } else if member.is(CONTEXT, 2) {
            let (&unused_bits, bits) = primitive(&member, "arbitrary [2]")?
                .split_first()
                .ok_or_else(|| DialogueError::new("arbitrary [2] BIT STRING has no content"))?;
            if unused_bits > 7 || (bits.is_empty() && unused_bits != 0) {
                return Err(DialogueError::new(
                    "arbitrary [2] BIT STRING has an invalid count of unused bits",
                ));
            }
            ExternalEncoding::Arbitrary {
                unused_bits,
                bits: bits.to_vec(),
            }
        } else {
            return Err(DialogueError::new(format!(
                "EXTERNAL holds {} where its encoding is expected",
                member.tag()
            )));
        };
        nothing_left(members.next(), "EXTERNAL")?;
        Ok(Self {
            direct_reference,
            indirect_reference,
            data_value_descriptor,
            encoding,
        })
    }
}

/// A typed dialogue control PDU (3.2/Q.773).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialoguePdu {
    /// `AARQ-apdu`, the dialogue request, in a Begin.
    Aarq {
        /// Protocol version (DEFAULT `version1`).
        protocol_version: ProtocolVersion,
        /// The proposed application context.
        application_context_name: ApplicationContextName,
        /// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL`.
        user_information: Option<Vec<External>>,
    },
    /// `AARE-apdu`, the dialogue response, in the first backward Continue or
    /// End, or in an Abort when the dialogue is refused.
    Aare {
        /// Protocol version (DEFAULT `version1`).
        protocol_version: ProtocolVersion,
        /// The application context.
        application_context_name: ApplicationContextName,
        /// The associate result.
        result: AssociateResult,
        /// The result source diagnostic.
        result_source_diagnostic: AssociateSourceDiagnostic,
        /// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL`.
        user_information: Option<Vec<External>>,
    },
    /// `ABRT-apdu`, a dialogue abort, in an Abort.
    Abrt {
        /// Who aborted.
        abort_source: AbortSource,
        /// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL`.
        user_information: Option<Vec<External>>,
    },
    /// `AUDT-apdu`, the unstructured dialogue PDU, in a Unidirectional.
    Audt {
        /// Protocol version (DEFAULT `version1`).
        protocol_version: ProtocolVersion,
        /// The application context.
        application_context_name: ApplicationContextName,
        /// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL`.
        user_information: Option<Vec<External>>,
    },
}

impl DialoguePdu {
    /// The abstract syntax that carries this PDU.
    fn abstract_syntax(&self) -> &'static Oid {
        match self {
            Self::Audt { .. } => UNIDIALOGUE_AS,
            _ => DIALOGUE_AS,
        }
    }

    /// The PDU's own encoding, starting at its APPLICATION tag.
    fn encode(&self) -> Vec<u8> {
        match self {
            Self::Aarq {
                protocol_version,
                application_context_name,
                user_information,
            }
            | Self::Audt {
                protocol_version,
                application_context_name,
                user_information,
            } => {
                let mut body = ber::tlv(0x80, &protocol_version.content());
                body.extend(application_context_name_tlv(application_context_name));
                body.extend(user_information_tlv(user_information));
                ber::tlv(0x60, &body)
            }
            Self::Aare {
                protocol_version,
                application_context_name,
                result,
                result_source_diagnostic,
                user_information,
            } => {
                let mut body = ber::tlv(0x80, &protocol_version.content());
                body.extend(application_context_name_tlv(application_context_name));
                body.extend(ber::tlv(0xa2, &integer_tlv(result.value())));
                let diagnostic = match *result_source_diagnostic {
                    AssociateSourceDiagnostic::DialogueServiceUser(value) => {
                        ber::tlv(0xa1, &integer_tlv(value))
                    }
                    AssociateSourceDiagnostic::DialogueServiceProvider(value) => {
                        ber::tlv(0xa2, &integer_tlv(value))
                    }
                };
                body.extend(ber::tlv(0xa3, &diagnostic));
                body.extend(user_information_tlv(user_information));
                ber::tlv(0x61, &body)
            }
            Self::Abrt {
                abort_source,
                user_information,
            } => {
                let mut body = ber::tlv(0x80, &ber::integer_content(abort_source.value()));
                body.extend(user_information_tlv(user_information));
                ber::tlv(0x64, &body)
            }
        }
    }
}

/// What a well-formed dialogue portion holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueContent {
    /// A dialogue control PDU this crate models.
    Pdu(DialoguePdu),
    /// A well-formed `EXTERNAL` carrying something else: user information in
    /// a user-defined abstract syntax, or an RLRQ / RLRE PDU (defined by Q.773
    /// "for completeness only", and not used).
    Unmodelled(External),
}

impl DialogueContent {
    /// The dialogue control PDU, if that is what the portion holds.
    pub fn pdu(self) -> Option<DialoguePdu> {
        match self {
            Self::Pdu(pdu) => Some(pdu),
            Self::Unmodelled(_) => None,
        }
    }
}

// ── Writers ──────────────────────────────────────────────────────────────────

/// OBJECT IDENTIFIER content octets (X.690 8.19): the first two arcs share
/// one subidentifier, every subidentifier is base 128 with the top bit set on
/// all octets but the last.
fn oid_content(oid: &Oid) -> Vec<u8> {
    fn subidentifier(out: &mut Vec<u8>, value: u64) {
        let mut groups = [0u8; 10];
        let mut count = 0;
        let mut rest = value;
        loop {
            groups[count] = (rest & 0x7f) as u8;
            count += 1;
            rest >>= 7;
            if rest == 0 {
                break;
            }
        }
        for index in (0..count).rev() {
            let more = if index == 0 { 0x00 } else { 0x80 };
            out.push(groups[index] | more);
        }
    }

    let mut out = Vec::new();
    let mut arcs = oid.iter().map(|&arc| u64::from(arc));
    // An Oid has at least two arcs; rasn does not build one with fewer.
    let first = arcs.next().unwrap_or(0);
    let second = arcs.next().unwrap_or(0);
    subidentifier(&mut out, first * 40 + second);
    for arc in arcs {
        subidentifier(&mut out, arc);
    }
    out
}

/// A universal INTEGER, tag and all.
fn integer_tlv(value: i64) -> Vec<u8> {
    ber::tlv(0x02, &ber::integer_content(value))
}

/// `application-context-name [1] OBJECT IDENTIFIER`: an explicit tag.
fn application_context_name_tlv(name: &Oid) -> Vec<u8> {
    ber::tlv(0xa1, &ber::tlv(0x06, &oid_content(name)))
}

/// `user-information [30] IMPLICIT SEQUENCE OF EXTERNAL OPTIONAL`.
fn user_information_tlv(user_information: &Option<Vec<External>>) -> Vec<u8> {
    match user_information {
        Some(externals) => {
            let content: Vec<u8> = externals.iter().flat_map(External::encode).collect();
            ber::tlv(0xbe, &content)
        }
        None => Vec::new(),
    }
}

// ── Readers ──────────────────────────────────────────────────────────────────

/// The one element `bytes` consists of.
fn sole_element<'a>(bytes: &'a [u8], what: &str) -> Parsed<Element<'a>> {
    let found = ber::element(bytes).map_err(|e| DialogueError::new(format!("{what}: {e}")))?;
    if !found.rest.is_empty() {
        return Err(DialogueError::new(format!(
            "{what}: {} octets follow the value",
            found.rest.len()
        )));
    }
    Ok(found)
}

fn children<'a>(content: &'a [u8], what: &str) -> Parsed<Vec<Element<'a>>> {
    ber::elements(content).map_err(|e| DialogueError::new(format!("{what}: {e}")))
}

fn nothing_left(next: Option<Element<'_>>, what: &str) -> Parsed<()> {
    match next {
        None => Ok(()),
        Some(extra) => Err(DialogueError::new(format!(
            "{what} holds {} which is not expected there",
            extra.tag()
        ))),
    }
}

/// The content of an element that has to be primitive (4.1.1/Q.773: OCTET
/// STRING and BIT STRING values are primitive; INTEGER and OBJECT IDENTIFIER
/// always are).
fn primitive<'a>(member: &Element<'a>, what: &str) -> Parsed<&'a [u8]> {
    if member.constructed {
        return Err(DialogueError::new(format!(
            "{what} is constructed, it has to be primitive"
        )));
    }
    Ok(member.content)
}

fn integer(member: &Element<'_>, what: &str) -> Parsed<i64> {
    ber::integer(primitive(member, what)?).map_err(|e| DialogueError::new(format!("{what}: {e}")))
}

fn object_identifier(member: &Element<'_>, what: &str) -> Parsed<ObjectIdentifier> {
    let content = primitive(member, what)?;
    let malformed = || DialogueError::new(format!("{what}: malformed OBJECT IDENTIFIER"));

    let mut subidentifiers = Vec::new();
    let mut value: u32 = 0;
    let mut pending = false;
    for &octet in content {
        if !pending && octet == 0x80 {
            // X.690 8.19.2: a subidentifier does not start with 0x80.
            return Err(malformed());
        }
        value = value
            .checked_mul(128)
            .and_then(|v| v.checked_add(u32::from(octet & 0x7f)))
            .ok_or_else(malformed)?;
        pending = octet & 0x80 != 0;
        if !pending {
            subidentifiers.push(value);
            value = 0;
        }
    }
    let Some((&first, others)) = subidentifiers.split_first() else {
        return Err(malformed());
    };
    if pending {
        return Err(malformed());
    }
    // X.690 8.19.4: the first subidentifier is 40 * arc1 + arc2, and arc1 is
    // 0, 1 or 2.
    let mut arcs = match first {
        0..=39 => vec![0, first],
        40..=79 => vec![1, first - 40],
        _ => vec![2, first - 80],
    };
    arcs.extend_from_slice(others);
    ObjectIdentifier::new(arcs).ok_or_else(malformed)
}

/// The value inside an explicit context tag: `[n] { one element }`.
fn explicit<'a>(member: &Element<'a>, what: &str) -> Parsed<Element<'a>> {
    if !member.constructed {
        return Err(DialogueError::new(format!(
            "{what} is primitive, it has to be constructed"
        )));
    }
    sole_element(member.content, what)
}

type Members<'a> = std::iter::Peekable<std::vec::IntoIter<Element<'a>>>;

fn take_protocol_version(members: &mut Members<'_>) -> Parsed<ProtocolVersion> {
    match members.next_if(|m| m.is(CONTEXT, 0)) {
        Some(member) => ProtocolVersion::from_content(primitive(&member, "protocol-version [0]")?),
        // DEFAULT { version1 }.
        None => Ok(ProtocolVersion::Version1),
    }
}

fn take_application_context_name(members: &mut Members<'_>) -> Parsed<ApplicationContextName> {
    let what = "application-context-name [1]";
    let member = members
        .next_if(|m| m.is(CONTEXT, 1))
        .ok_or_else(|| DialogueError::new(format!("{what} is missing")))?;
    let inner = explicit(&member, what)?;
    if !inner.is(UNIVERSAL, 6) {
        return Err(DialogueError::new(format!(
            "{what} holds {}, not an OBJECT IDENTIFIER",
            inner.tag()
        )));
    }
    object_identifier(&inner, what)
}

fn take_user_information(members: &mut Members<'_>) -> Parsed<Option<Vec<External>>> {
    let what = "user-information [30]";
    let Some(member) = members.next_if(|m| m.is(CONTEXT, 30)) else {
        return Ok(None);
    };
    if !member.constructed {
        return Err(DialogueError::new(format!(
            "{what} is primitive, it has to be constructed"
        )));
    }
    children(member.content, what)?
        .iter()
        .map(External::from_element)
        .collect::<Parsed<Vec<_>>>()
        .map(Some)
}

/// An INTEGER behind an explicit context tag.
fn explicit_integer(member: &Element<'_>, what: &str) -> Parsed<i64> {
    let inner = explicit(member, what)?;
    if !inner.is(UNIVERSAL, 2) {
        return Err(DialogueError::new(format!(
            "{what} holds {}, not an INTEGER",
            inner.tag()
        )));
    }
    integer(&inner, what)
}

fn parse_aarq_or_audt(pdu: &Element<'_>, unstructured: bool) -> Parsed<DialoguePdu> {
    let name = if unstructured { "AUDT" } else { "AARQ" };
    let mut members = children(pdu.content, name)?.into_iter().peekable();
    let protocol_version = take_protocol_version(&mut members)?;
    let application_context_name = take_application_context_name(&mut members)?;
    let user_information = take_user_information(&mut members)?;
    nothing_left(members.next(), name)?;
    Ok(if unstructured {
        DialoguePdu::Audt {
            protocol_version,
            application_context_name,
            user_information,
        }
    } else {
        DialoguePdu::Aarq {
            protocol_version,
            application_context_name,
            user_information,
        }
    })
}

fn parse_aare(pdu: &Element<'_>) -> Parsed<DialoguePdu> {
    let mut members = children(pdu.content, "AARE")?.into_iter().peekable();
    let protocol_version = take_protocol_version(&mut members)?;
    if protocol_version != ProtocolVersion::Version1 {
        // 3.2.3/Q.774: "the receipt of an AARE APDU with the version field set
        // to any value other than "version 1" shall be considered a syntax
        // error".
        return Err(DialogueError::new(
            "AARE protocol-version does not list version1",
        ));
    }
    let application_context_name = take_application_context_name(&mut members)?;

    let member = members
        .next_if(|m| m.is(CONTEXT, 2))
        .ok_or_else(|| DialogueError::new("AARE result [2] is missing"))?;
    let value = explicit_integer(&member, "result [2]")?;
    let result = AssociateResult::from_value(value)
        .ok_or_else(|| DialogueError::new(format!("AARE result {value} is not defined")))?;

    let what = "result-source-diagnostic [3]";
    let member = members
        .next_if(|m| m.is(CONTEXT, 3))
        .ok_or_else(|| DialogueError::new(format!("AARE {what} is missing")))?;
    let source = explicit(&member, what)?;
    let result_source_diagnostic = if source.is(CONTEXT, 1) {
        AssociateSourceDiagnostic::DialogueServiceUser(explicit_integer(
            &source,
            "dialogue-service-user [1]",
        )?)
    } else if source.is(CONTEXT, 2) {
        AssociateSourceDiagnostic::DialogueServiceProvider(explicit_integer(
            &source,
            "dialogue-service-provider [2]",
        )?)
    } else {
        return Err(DialogueError::new(format!(
            "{what} holds {}, not dialogue-service-user [1] or dialogue-service-provider [2]",
            source.tag()
        )));
    };

    let user_information = take_user_information(&mut members)?;
    nothing_left(members.next(), "AARE")?;
    Ok(DialoguePdu::Aare {
        protocol_version,
        application_context_name,
        result,
        result_source_diagnostic,
        user_information,
    })
}

fn parse_abrt(pdu: &Element<'_>) -> Parsed<DialoguePdu> {
    let mut members = children(pdu.content, "ABRT")?.into_iter().peekable();
    let member = members
        .next_if(|m| m.is(CONTEXT, 0))
        .ok_or_else(|| DialogueError::new("ABRT abort-source [0] is missing"))?;
    let value = integer(&member, "abort-source [0]")?;
    let abort_source = AbortSource::from_value(value)
        .ok_or_else(|| DialogueError::new(format!("ABRT abort-source {value} is not defined")))?;
    let user_information = take_user_information(&mut members)?;
    nothing_left(members.next(), "ABRT")?;
    Ok(DialoguePdu::Abrt {
        abort_source,
        user_information,
    })
}

/// Dialogue Portion: the `EXTERNAL` of `DialoguePortion ::= [APPLICATION 11]
/// EXTERNAL` (3.1/Q.773).
///
/// The message structs tag this member `[APPLICATION 11]`, which puts `6B
/// len` around [`external`](Self::external); the member itself holds the BER
/// of the `EXTERNAL`, from its tag `28` on.
///
/// Build one with [`aarq`](Self::aarq), [`aare_accept`](Self::aare_accept),
/// [`aare_reject`](Self::aare_reject), [`abrt`](Self::abrt),
/// [`audt`](Self::audt), [`from_pdu`](Self::from_pdu) or
/// [`from_external`](Self::from_external), and read one with
/// [`parse`](Self::parse). The decoder has already parsed the dialogue portion
/// of every message it returns as fully understood, so on such a message
/// `parse` does not fail.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct DialoguePortion {
    /// The BER encoding of the `EXTERNAL`.
    pub external: Any,
}

impl DialoguePortion {
    /// Build a dialogue portion from a typed [`DialoguePdu`]: an `EXTERNAL`
    /// naming the dialogue or unidialogue abstract syntax, with the PDU as a
    /// single-ASN1-type.
    pub fn from_pdu(pdu: &DialoguePdu) -> Self {
        let mut content = ber::tlv(0x06, &oid_content(pdu.abstract_syntax()));
        content.extend(ber::tlv(0xa0, &pdu.encode()));
        Self {
            external: Any::new(ber::tlv(0x28, &content)),
        }
    }

    /// Build a dialogue portion from any `EXTERNAL`, for user information in
    /// a user-defined abstract syntax.
    pub fn from_external(external: &External) -> Self {
        Self {
            external: Any::new(external.encode()),
        }
    }

    /// An **AARQ** proposing the application context `ac` (protocol version
    /// `version1`, no user information), for a Begin.
    pub fn aarq(ac: &Oid) -> Self {
        Self::from_pdu(&DialoguePdu::Aarq {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            user_information: None,
        })
    }

    /// An accepting **AARE** for `ac`: result `accepted (0)`,
    /// result-source-diagnostic `dialogue-service-user: null (0)`. For the
    /// first backward Continue or End.
    pub fn aare_accept(ac: &Oid) -> Self {
        Self::from_pdu(&DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            result: AssociateResult::Accepted,
            result_source_diagnostic: AssociateSourceDiagnostic::USER_NULL,
            user_information: None,
        })
    }

    /// A refusing **AARE** for `ac`: result `reject-permanent (1)` with the
    /// given diagnostic. It travels as the user abort information of an Abort
    /// (3.2.1.2/Q.774: `application-context-name-not-supported` or a
    /// `dialogue-refused` by the TC-user).
    pub fn aare_reject(ac: &Oid, diagnostic: AssociateSourceDiagnostic) -> Self {
        Self::from_pdu(&DialoguePdu::Aare {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            result: AssociateResult::RejectedPermanent,
            result_source_diagnostic: diagnostic,
            user_information: None,
        })
    }

    /// An **ABRT** with the given abort source and no user information.
    pub fn abrt(source: AbortSource) -> Self {
        Self::from_pdu(&DialoguePdu::Abrt {
            abort_source: source,
            user_information: None,
        })
    }

    /// An **AUDT** naming the application context `ac`, for a Unidirectional.
    pub fn audt(ac: &Oid) -> Self {
        Self::from_pdu(&DialoguePdu::Audt {
            protocol_version: ProtocolVersion::Version1,
            application_context_name: ac.into(),
            user_information: None,
        })
    }

    /// What the component sub-layer sends for an incorrect dialogue portion
    /// (3.2.2.1/Q.774): "a TR-U-ABORT request primitive is issued to the
    /// transaction sub-layer with an ABRT APDU as user data. The abort-source
    /// field of the ABRT APDU is set to "dialogue-service-provider" and the
    /// user information field is absent."
    pub fn abnormal_dialogue() -> Self {
        Self::abrt(AbortSource::DialogueServiceProvider)
    }

    /// What the component sub-layer sends for an AARQ whose protocol version
    /// does not list version 1 (3.2.3/Q.774): an AARE with "protocol-version =
    /// version 1; application-context-name = the one received in the AARQ
    /// APDU; result = reject (permanent); result-source-diagnostic =
    /// dialogue-service-provider (no-common-dialogue-version);
    /// user-information = absent", as the user abort information of an Abort.
    pub fn no_common_dialogue_portion(ac: &Oid) -> Self {
        Self::aare_reject(
            ac,
            AssociateSourceDiagnostic::PROVIDER_NO_COMMON_DIALOGUE_PORTION,
        )
    }

    /// Read the dialogue portion.
    ///
    /// `Err` means the portion is malformed: it is not one well-formed
    /// `EXTERNAL`, or it names a Q.773 dialogue abstract syntax and what it
    /// carries is not a well-formed PDU of that syntax. Every element has to
    /// be accounted for; nothing is skipped.
    pub fn parse(&self) -> Result<DialogueContent, DialogueError> {
        let external = External::decode(self.external.as_bytes())?;
        let unstructured = match external.direct_reference.as_deref() {
            Some(syntax) if syntax == DIALOGUE_AS => false,
            Some(syntax) if syntax == UNIDIALOGUE_AS => true,
            _ => return Ok(DialogueContent::Unmodelled(external)),
        };

        // X.690 8.18: a BER-encoded value may travel in any of the three
        // forms, and Tables 33 and 34/Q.773 note that "the use of the
        // single-ASN.1-type constructor is only one possible encoding".
        let encoded: &[u8] = match &external.encoding {
            ExternalEncoding::SingleAsn1Type(value) => value.as_bytes(),
            ExternalEncoding::OctetAligned(octets) => octets,
            ExternalEncoding::Arbitrary {
                unused_bits: 0,
                bits,
            } => bits,
            ExternalEncoding::Arbitrary { .. } => {
                return Err(DialogueError::new(
                    "dialogue PDU carried as a bit string that is not a whole number of octets",
                ))
            }
        };
        let pdu = sole_element(encoded, "dialogue PDU")?;
        if pdu.class != APPLICATION || !pdu.constructed {
            return Err(DialogueError::new(format!(
                "{} is not a dialogue PDU",
                pdu.tag()
            )));
        }
        let parsed = match (unstructured, pdu.number) {
            (false, 0) => parse_aarq_or_audt(&pdu, false)?,
            (false, 1) => parse_aare(&pdu)?,
            (false, 4) => parse_abrt(&pdu)?,
            // RLRQ and RLRE: "currently not used", "included for completeness
            // only" (3.2.1/Q.773).
            (false, 2 | 3) => return Ok(DialogueContent::Unmodelled(external)),
            (true, 0) => parse_aarq_or_audt(&pdu, true)?,
            _ => {
                return Err(DialogueError::new(format!(
                    "{} is not a PDU of the abstract syntax the EXTERNAL names",
                    pdu.tag()
                )))
            }
        };
        Ok(DialogueContent::Pdu(parsed))
    }

    /// The dialogue control PDU: `Ok(None)` for a well-formed portion that
    /// carries something this crate does not model, `Err` for a malformed
    /// portion. See [`parse`](Self::parse).
    pub fn dialogue_pdu(&self) -> Result<Option<DialoguePdu>, DialogueError> {
        self.parse().map(DialogueContent::pdu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two application contexts of 3GPP TS 29.002 and TS 29.078, as examples.
    const SHORT_MSG_GATEWAY_V3: &[u32] = &[0, 4, 0, 0, 1, 0, 20, 3];
    const GSMSSF_SCF_GENERIC_V3: &[u32] = &[0, 4, 0, 0, 1, 21, 3, 4];

    fn oid(arcs: &'static [u32]) -> &'static Oid {
        Oid::const_new(arcs)
    }

    #[test]
    fn oid_content_follows_x690() {
        // 0.4.0.0.1.0.20.3: first subidentifier 0*40 + 4.
        assert_eq!(
            oid_content(oid(SHORT_MSG_GATEWAY_V3)),
            vec![0x04, 0x00, 0x00, 0x01, 0x00, 0x14, 0x03]
        );
        // 0.0.17.773.1.1.1: 773 = 6*128 + 5, so 86 05.
        assert_eq!(
            oid_content(DIALOGUE_AS),
            vec![0x00, 0x11, 0x86, 0x05, 0x01, 0x01, 0x01]
        );
        // 2.999.1: the first subidentifier is 2*40 + 999 = 1079 = 8*128 + 55
        // and takes two octets.
        assert_eq!(oid_content(oid(&[2, 999, 1])), vec![0x88, 0x37, 0x01]);
        // 1.2.840.113549: 1*40 + 2 = 2a, 840 = 86 48, 113549 = 86 f7 0d.
        assert_eq!(
            oid_content(oid(&[1, 2, 840, 113549])),
            vec![0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d]
        );
    }

    #[test]
    fn object_identifiers_are_read_back() {
        for arcs in [
            SHORT_MSG_GATEWAY_V3,
            GSMSSF_SCF_GENERIC_V3,
            DIALOGUE_AS_OID,
            UNIDIALOGUE_AS_OID,
            &[2, 999, 1],
            &[1, 2, 840, 113549],
            &[2, 5],
        ] {
            let tlv = ber::tlv(0x06, &oid_content(oid(arcs)));
            let element = ber::element(&tlv).unwrap();
            let read = object_identifier(&element, "test").unwrap();
            assert_eq!(&*read, oid(arcs), "{arcs:?}");
        }
    }

    #[test]
    fn malformed_object_identifiers_are_refused() {
        for content in [
            &[][..],             // no content
            &[0x04, 0x86],       // a subidentifier that does not end
            &[0x04, 0x80, 0x01], // a subidentifier starting with 0x80
        ] {
            let tlv = ber::tlv(0x06, content);
            let element = ber::element(&tlv).unwrap();
            assert!(object_identifier(&element, "test").is_err(), "{content:?}");
        }
    }

    /// The AARQ octets, assembled here piece by piece: EXTERNAL (28), the
    /// dialogue-as OID, [0], AARQ (60), protocol-version,
    /// application-context-name.
    #[test]
    fn aarq_bytes_are_byte_exact() {
        let dp = DialoguePortion::aarq(oid(SHORT_MSG_GATEWAY_V3));
        let expected = [
            0x28, 0x1c, // EXTERNAL, 28
            0x06, 0x07, 0x00, 0x11, 0x86, 0x05, 0x01, 0x01, 0x01, // dialogue-as
            0xa0, 0x11, // single-ASN1-type, 17
            0x60, 0x0f, // AARQ, 15
            0x80, 0x02, 0x07, 0x80, // protocol-version
            0xa1, 0x09, 0x06, 0x07, 0x04, 0x00, 0x00, 0x01, 0x00, 0x14, 0x03,
        ];
        assert_eq!(dp.external.as_bytes(), &expected);
    }

    #[test]
    fn aare_bytes_are_byte_exact() {
        let dp = DialoguePortion::aare_accept(oid(GSMSSF_SCF_GENERIC_V3));
        let expected = [
            0x28, 0x28, // EXTERNAL, 40
            0x06, 0x07, 0x00, 0x11, 0x86, 0x05, 0x01, 0x01, 0x01, // dialogue-as
            0xa0, 0x1d, // single-ASN1-type, 29
            0x61, 0x1b, // AARE, 27
            0x80, 0x02, 0x07, 0x80, // protocol-version
            0xa1, 0x09, 0x06, 0x07, 0x04, 0x00, 0x00, 0x01, 0x15, 0x03, 0x04, // context
            0xa2, 0x03, 0x02, 0x01, 0x00, // result accepted (0)
            0xa3, 0x05, 0xa1, 0x03, 0x02, 0x01, 0x00, // dialogue-service-user null (0)
        ];
        assert_eq!(dp.external.as_bytes(), &expected);
    }

    #[test]
    fn every_pdu_reads_back_as_built() {
        let user_information = Some(vec![External::single_asn1_type(
            oid(&[2, 999, 1]),
            Any::new(vec![0x04, 0x01, 0x2a]),
        )]);
        let pdus = [
            DialoguePdu::Aarq {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(SHORT_MSG_GATEWAY_V3).into(),
                user_information: None,
            },
            DialoguePdu::Aarq {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(SHORT_MSG_GATEWAY_V3).into(),
                user_information: user_information.clone(),
            },
            DialoguePdu::Aare {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(GSMSSF_SCF_GENERIC_V3).into(),
                result: AssociateResult::Accepted,
                result_source_diagnostic: AssociateSourceDiagnostic::USER_NULL,
                user_information: None,
            },
            DialoguePdu::Aare {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(SHORT_MSG_GATEWAY_V3).into(),
                result: AssociateResult::RejectedPermanent,
                result_source_diagnostic:
                    AssociateSourceDiagnostic::PROVIDER_NO_COMMON_DIALOGUE_PORTION,
                user_information: user_information.clone(),
            },
            DialoguePdu::Abrt {
                abort_source: AbortSource::DialogueServiceUser,
                user_information: user_information.clone(),
            },
            DialoguePdu::Abrt {
                abort_source: AbortSource::DialogueServiceProvider,
                user_information: None,
            },
            DialoguePdu::Audt {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(SHORT_MSG_GATEWAY_V3).into(),
                user_information,
            },
        ];
        for pdu in pdus {
            let portion = DialoguePortion::from_pdu(&pdu);
            assert_eq!(portion.parse(), Ok(DialogueContent::Pdu(pdu.clone())));
            assert_eq!(portion.dialogue_pdu(), Ok(Some(pdu)));
        }
    }

    #[test]
    fn builders_match_from_pdu() {
        assert_eq!(
            DialoguePortion::aarq(oid(SHORT_MSG_GATEWAY_V3)),
            DialoguePortion::from_pdu(&DialoguePdu::Aarq {
                protocol_version: ProtocolVersion::Version1,
                application_context_name: oid(SHORT_MSG_GATEWAY_V3).into(),
                user_information: None,
            })
        );
        assert_eq!(
            DialoguePortion::abnormal_dialogue(),
            DialoguePortion::from_pdu(&DialoguePdu::Abrt {
                abort_source: AbortSource::DialogueServiceProvider,
                user_information: None,
            })
        );
    }

    #[test]
    fn an_external_reads_back_in_each_encoding() {
        let externals = [
            External::single_asn1_type(oid(&[2, 999, 1]), Any::new(vec![0x05, 0x00])),
            External {
                direct_reference: None,
                indirect_reference: Some(3),
                data_value_descriptor: Some(b"note".to_vec()),
                encoding: ExternalEncoding::OctetAligned(vec![0x01, 0x02]),
            },
            External {
                direct_reference: Some(oid(&[2, 999, 1]).into()),
                indirect_reference: Some(-1),
                data_value_descriptor: None,
                encoding: ExternalEncoding::Arbitrary {
                    unused_bits: 3,
                    bits: vec![0xa8],
                },
            },
        ];
        for external in externals {
            assert_eq!(External::decode(&external.encode()), Ok(external));
        }
    }

    #[test]
    fn something_that_is_not_an_external_is_malformed() {
        // 1.0.0 answered "no dialogue PDU" for this. It is an error.
        let portion = DialoguePortion {
            external: Any::new(vec![0x30, 0x03, 0x06, 0x01, 0x2a]),
        };
        assert!(portion.parse().is_err());
        assert!(portion.dialogue_pdu().is_err());
    }
}
