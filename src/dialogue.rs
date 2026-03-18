use rasn::prelude::*;

/// Dialogue Portion — wraps the EXTERNAL type per ITU-T Q.773.
///
/// With implicit APPLICATION 11 tagging on the parent struct field,
/// rasn produces `6B len { EXTERNAL_bytes }` which is wire-correct.
///
/// The `external` field should contain BER-encoded EXTERNAL content
/// (tag 0x28, containing direct-reference OID + encoding).
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct DialoguePortion {
    /// The EXTERNAL type containing dialogue PDU (AARQ/AARE/ABRT).
    pub external: rasn::types::Any,
}

/// Application Context Name — an OID identifying the MAP operation set.
pub type ApplicationContextName = rasn::types::ObjectIdentifier;
