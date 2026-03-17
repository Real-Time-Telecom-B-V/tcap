use rasn::prelude::*;

/// Dialogue Portion types per ITU-T Q.773 Section 3.1.
///
/// The dialogue portion is optional in TCAP messages and carries
/// application context negotiation information.
/// Dialogue Portion — wraps the external type containing dialogue PDUs.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct DialoguePortion {
    /// The dialogue PDU encoded as opaque bytes.
    /// Contains either AARQ-apdu, AARE-apdu, or ABRT-apdu.
    pub direct_reference: Option<rasn::types::ObjectIdentifier>,
    pub encoding: rasn::types::Any,
}

/// Application Context Name — an OID identifying the MAP operation set.
pub type ApplicationContextName = rasn::types::ObjectIdentifier;
