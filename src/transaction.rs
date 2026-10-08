use std::fmt;

use rasn::prelude::*;

use crate::component::Component;
use crate::dialogue::DialoguePortion;
use crate::types::{PAbortCause, TransactionId};

/// TCAP message types per ITU-T Q.773 clause 3.1.
///
/// Each message type has an application-class tag (Table 8/Q.773):
/// - Unidirectional: `[APPLICATION 1]` (`61`)
/// - Begin:          `[APPLICATION 2]` (`62`)
/// - End:            `[APPLICATION 4]` (`64`)
/// - Continue:       `[APPLICATION 5]` (`65`)
/// - Abort:          `[APPLICATION 7]` (`67`)
///
/// The derived `rasn::Decode` is not the crate's decoder: `rasn` returns a
/// component portion without the components it could not read. Use
/// [`crate::decode`] or [`crate::decode_detailed`].
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

/// The message type, which is the application tag of a [`TcapMessage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    Unidirectional,
    Begin,
    End,
    Continue,
    Abort,
}

impl MessageType {
    /// The application tag number.
    pub fn tag(self) -> u8 {
        match self {
            Self::Unidirectional => 1,
            Self::Begin => 2,
            Self::End => 4,
            Self::Continue => 5,
            Self::Abort => 7,
        }
    }

    /// The message type for an application tag number, if Q.773 defines one.
    pub fn from_tag(tag: u32) -> Option<Self> {
        match tag {
            1 => Some(Self::Unidirectional),
            2 => Some(Self::Begin),
            4 => Some(Self::End),
            5 => Some(Self::Continue),
            7 => Some(Self::Abort),
            _ => None,
        }
    }
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unidirectional => "Unidirectional",
            Self::Begin => "Begin",
            Self::End => "End",
            Self::Continue => "Continue",
            Self::Abort => "Abort",
        })
    }
}

impl TcapMessage {
    /// The message type.
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::Unidirectional(_) => MessageType::Unidirectional,
            Self::Begin(_) => MessageType::Begin,
            Self::End(_) => MessageType::End,
            Self::Continue(_) => MessageType::Continue,
            Self::Abort(_) => MessageType::Abort,
        }
    }

    /// The originating transaction ID (Begin and Continue carry one).
    pub fn otid(&self) -> Option<&TransactionId> {
        match self {
            Self::Begin(begin) => Some(&begin.otid),
            Self::Continue(cont) => Some(&cont.otid),
            _ => None,
        }
    }

    /// The destination transaction ID (End, Continue and Abort carry one).
    pub fn dtid(&self) -> Option<&TransactionId> {
        match self {
            Self::End(end) => Some(&end.dtid),
            Self::Continue(cont) => Some(&cont.dtid),
            Self::Abort(abort) => Some(&abort.dtid),
            _ => None,
        }
    }

    /// The dialogue portion. For an Abort this is the user abort information.
    pub fn dialogue_portion(&self) -> Option<&DialoguePortion> {
        match self {
            Self::Unidirectional(uni) => uni.dialogue_portion.as_ref(),
            Self::Begin(begin) => begin.dialogue_portion.as_ref(),
            Self::End(end) => end.dialogue_portion.as_ref(),
            Self::Continue(cont) => cont.dialogue_portion.as_ref(),
            Self::Abort(abort) => match &abort.reason {
                Some(AbortReason::UAbort(portion)) => Some(portion),
                _ => None,
            },
        }
    }

    /// The components, empty when the message has no component portion.
    pub fn components(&self) -> &[Component] {
        match self {
            Self::Unidirectional(uni) => &uni.components,
            Self::Begin(begin) => begin.components.as_deref().unwrap_or(&[]),
            Self::End(end) => end.components.as_deref().unwrap_or(&[]),
            Self::Continue(cont) => cont.components.as_deref().unwrap_or(&[]),
            Self::Abort(_) => &[],
        }
    }
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
            Self::Abort(a) => match &a.reason {
                Some(AbortReason::PAbort(cause)) => {
                    write!(f, "Abort [dtid={}, p-abort={cause}]", hex::encode(&a.dtid))
                }
                Some(AbortReason::UAbort(_)) => {
                    write!(f, "Abort [dtid={}, u-abort]", hex::encode(&a.dtid))
                }
                None => write!(f, "Abort [dtid={}]", hex::encode(&a.dtid)),
            },
        }
    }
}

/// Unidirectional message — no transaction, fire-and-forget.
///
/// The component portion is mandatory and holds at least one component
/// (`ComponentPortion ::= SEQUENCE SIZE (1..MAX) OF Component`).
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Unidirectional {
    #[rasn(tag(application, 11))]
    pub dialogue_portion: Option<DialoguePortion>,
    #[rasn(tag(application, 12))]
    pub components: Vec<Component>,
}

/// Begin message — initiates a new transaction.
///
/// `components`, when present, holds at least one component; a message
/// without components has `None`.
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

/// Why a transaction is aborted (3.1/Q.773):
///
/// ```text
/// reason CHOICE { p-abortCause P-AbortCause,
///                 u-abortCause DialoguePortion } OPTIONAL
/// ```
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum AbortReason {
    /// `p-abortCause`: the transaction sub-layer aborted (`4A 01 nn`).
    #[rasn(tag(application, 10))]
    PAbort(PAbortCause),
    /// `u-abortCause`: the component sub-layer or the TC-user aborted. The
    /// dialogue portion holds an ABRT or AARE APDU, or user information in a
    /// user-defined abstract syntax (`6B ..`).
    #[rasn(tag(application, 11))]
    UAbort(DialoguePortion),
}

/// Abort message — aborts a transaction.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
pub struct Abort {
    /// Destination Transaction ID.
    #[rasn(tag(application, 9))]
    pub dtid: TransactionId,
    /// The P-Abort cause, or the user abort information. `None` is an abort
    /// by the TC-user that carries no information.
    pub reason: Option<AbortReason>,
}

impl Abort {
    /// An Abort by the transaction sub-layer (3.3.4/Q.774): "The transaction
    /// sub-layer should form an Abort message with an appropriate P-Abort
    /// cause information element and transmit it to the originating end."
    ///
    /// `dtid` is the originating transaction ID of the message being
    /// answered. For a message whose destination transaction ID is not
    /// assigned, the cause is [`PAbortCause::UNRECOGNIZED_TRANSACTION_ID`].
    pub fn p_abort(dtid: TransactionId, cause: PAbortCause) -> Self {
        Self {
            dtid,
            reason: Some(AbortReason::PAbort(cause)),
        }
    }

    /// An Abort by the TC-user or the component sub-layer, with the user
    /// abort information, if any.
    pub fn u_abort(dtid: TransactionId, information: Option<DialoguePortion>) -> Self {
        Self {
            dtid,
            reason: information.map(AbortReason::UAbort),
        }
    }

    /// The P-Abort cause, when the transaction sub-layer aborted.
    pub fn p_abort_cause(&self) -> Option<PAbortCause> {
        match self.reason {
            Some(AbortReason::PAbort(cause)) => Some(cause),
            _ => None,
        }
    }
}
