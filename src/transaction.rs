use std::fmt;

use rasn::prelude::*;

use crate::component::Component;
use crate::dialogue::DialoguePortion;
use crate::types::TransactionId;

/// TCAP Transaction types per ITU-T Q.773.
///
/// Each transaction type uses application-class tags:
/// - Begin:         [APPLICATION 2]
/// - End:           [APPLICATION 4]
/// - Continue:      [APPLICATION 5]
/// - Abort:         [APPLICATION 7]
/// - Unidirectional:[APPLICATION 1]
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum TcapMessage {
    #[rasn(tag(application, 1))]
    Unidirectional(Unidirectional),
    #[rasn(tag(application, 2))]
    Begin(Begin),
    #[rasn(tag(application, 4))]
    End(End),
    #[rasn(tag(application, 5))]
    Continue(Continue),
    #[rasn(tag(application, 7))]
    Abort(Abort),
}

impl fmt::Display for TcapMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unidirectional(u) => {
                write!(f, "Unidirectional [{} components]", u.components.len())
            }
            Self::Begin(b) => {
                write!(
                    f,
                    "Begin [otid={}, {} components]",
                    hex::encode(&b.otid),
                    b.components.as_ref().map_or(0, |c| c.len())
                )
            }
            Self::End(e) => {
                write!(
                    f,
                    "End [dtid={}, {} components]",
                    hex::encode(&e.dtid),
                    e.components.as_ref().map_or(0, |c| c.len())
                )
            }
            Self::Continue(c) => {
                write!(
                    f,
                    "Continue [otid={}, dtid={}, {} components]",
                    hex::encode(&c.otid),
                    hex::encode(&c.dtid),
                    c.components.as_ref().map_or(0, |c| c.len())
                )
            }
            Self::Abort(a) => {
                write!(f, "Abort [dtid={}]", hex::encode(&a.dtid))
            }
        }
    }
}

/// Unidirectional message — no transaction, fire-and-forget.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Unidirectional {
    #[rasn(tag(application, 11))]
    pub dialogue_portion: Option<DialoguePortion>,
    #[rasn(tag(application, 12))]
    pub components: Vec<Component>,
}

/// Begin message — initiates a new transaction.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Begin {
    /// Originating Transaction ID.
    #[rasn(tag(application, 8))]
    pub otid: TransactionId,
    #[rasn(tag(application, 11))]
    pub dialogue_portion: Option<DialoguePortion>,
    #[rasn(tag(application, 12))]
    pub components: Option<Vec<Component>>,
}

/// End message — ends a transaction (response).
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct End {
    /// Destination Transaction ID.
    #[rasn(tag(application, 9))]
    pub dtid: TransactionId,
    #[rasn(tag(application, 11))]
    pub dialogue_portion: Option<DialoguePortion>,
    #[rasn(tag(application, 12))]
    pub components: Option<Vec<Component>>,
}

/// Continue message — continues an ongoing transaction.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Continue {
    /// Originating Transaction ID.
    #[rasn(tag(application, 8))]
    pub otid: TransactionId,
    /// Destination Transaction ID.
    #[rasn(tag(application, 9))]
    pub dtid: TransactionId,
    #[rasn(tag(application, 11))]
    pub dialogue_portion: Option<DialoguePortion>,
    #[rasn(tag(application, 12))]
    pub components: Option<Vec<Component>>,
}

/// Abort message — aborts a transaction.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Abort {
    /// Destination Transaction ID.
    #[rasn(tag(application, 9))]
    pub dtid: TransactionId,
    /// P-Abort cause or dialogue abort from user.
    #[rasn(tag(application, 10))]
    pub reason: Option<rasn::types::Any>,
}
