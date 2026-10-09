//! The decoder, and what it reports when a message is not fully understood.
//!
//! A TCAP receiver cannot stop at "this message is bad". Q.774 has it answer:
//! an Abort addressed with the originating transaction ID of the damaged
//! message, or a Reject carrying the invoke ID of the component that could
//! not be read. So the decoder works in the three stages the procedures
//! distinguish, and for a problem says which stage found it and hands back
//! what that stage's answer needs.
//!
//! 1. **Transaction portion** (transaction sub-layer, 3.3.4/Q.774): the
//!    message type, the transaction IDs, and the framing of the dialogue and
//!    component portions. A problem here has a P-Abort cause (2.3/Q.772).
//! 2. **Dialogue portion** (component sub-layer, 3.2.2.1/Q.774): a dialogue
//!    portion that is present must be well-formed. A problem here aborts the
//!    dialogue with an ABRT APDU, and the components of the message are
//!    discarded.
//! 3. **Component portion** (component sub-layer, 3.2.2.2/Q.774): components
//!    are read one at a time. A problem here has a general problem code
//!    (3.7.1/Q.772); the components before it stand, and "subsequent
//!    components in the message are discarded".
//!
//! Nothing is ever left out of a successful result: [`Decoded::Complete`]
//! means every octet of the input was accounted for.

use std::fmt;

use rasn::types::Any;

use crate::ber::{self, Element, Mismatch, APPLICATION, CONTEXT, UNIVERSAL};
use crate::component::{Component, ComponentType, Reject};
use crate::dialogue::DialoguePortion;
use crate::transaction::{
    Abort, AbortReason, Begin, Continue, End, MessageType, TcapMessage, Unidirectional,
};
use crate::types::{GeneralProblem, InvokeId, PAbortCause, TransactionId};

/// The TC sub-layer that detected a problem (Q.774 clause 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sublayer {
    /// The transaction sub-layer: the problem is in the transaction portion
    /// and is answered, if at all, with a P-Abort.
    Transaction,
    /// The component sub-layer: the problem is in the dialogue portion
    /// (answered with an ABRT APDU in an Abort) or in a component (answered
    /// with a Reject component).
    Component,
}

/// What is wrong with a message, in the terms Q.774 uses to decide the
/// response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The transaction portion cannot be understood (3.3.4/Q.774). `cause`
    /// is the P-Abort cause of 2.3/Q.772 that describes it.
    TransactionPortion { cause: PAbortCause },
    /// The dialogue portion is present and syntactically incorrect
    /// (3.2.2.1/Q.774). The local TC-user is told "abnormal dialogue", and
    /// any components in the message are discarded.
    DialoguePortion,
    /// A component cannot be understood (3.2.2.2/Q.774).
    Component {
        /// Position of the component in the component portion, from 0.
        index: usize,
        /// The component type, when its tag is one Q.773 defines.
        component_type: Option<ComponentType>,
        /// The invoke ID of the component, when it could be read: the first
        /// element of a component of a known type, if that is a one-octet
        /// INTEGER.
        invoke_id: Option<InvokeId>,
        /// The general problem that describes it (3.7.1/Q.772).
        problem: GeneralProblem,
    },
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransactionPortion { cause } => write!(f, "transaction portion, {cause}"),
            Self::DialoguePortion => f.write_str("dialogue portion, abnormal dialogue"),
            Self::Component { index, problem, .. } => {
                write!(f, "component {index}, {problem}")
            }
        }
    }
}

/// A message that was not fully understood, with what Q.774 needs to answer
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeProblem {
    /// What is wrong, and where.
    pub fault: Fault,
    /// The message type, when the message type tag is one Q.773 defines.
    pub message_type: Option<MessageType>,
    /// The originating transaction ID, when it is derivable: a primitive
    /// `[APPLICATION 8]` of 1 to 4 octets that could be reached. An Abort is
    /// addressed to it.
    pub otid: Option<TransactionId>,
    /// The destination transaction ID, when it is derivable. It names the
    /// local transaction the damaged message belongs to.
    pub dtid: Option<TransactionId>,
    /// For a component fault: the message as far as it was understood, that
    /// is the transaction portion, the dialogue portion and the components
    /// before the faulty one. Those components are to be processed; only
    /// "subsequent components in the message are discarded" (3.2.2.2/Q.774).
    /// `None` for the other faults, where the whole message is discarded.
    pub partial: Option<TcapMessage>,
    /// What exactly is wrong, for logs.
    pub detail: String,
}

impl DecodeProblem {
    /// The sub-layer that detected the problem.
    pub fn sublayer(&self) -> Sublayer {
        match self.fault {
            Fault::TransactionPortion { .. } => Sublayer::Transaction,
            Fault::DialoguePortion | Fault::Component { .. } => Sublayer::Component,
        }
    }

    /// The P-Abort cause, for a problem in the transaction portion.
    pub fn p_abort_cause(&self) -> Option<PAbortCause> {
        match self.fault {
            Fault::TransactionPortion { cause } => Some(cause),
            _ => None,
        }
    }

    /// The Abort message to send to the originator, when Q.774 calls for one.
    ///
    /// * A transaction portion fault in a Begin, a Continue or a message of
    ///   unknown type whose originating transaction ID is derivable: an Abort
    ///   with the P-Abort cause (3.3.4/Q.774, Table 7/Q.774).
    /// * A dialogue portion fault in a Begin or a Continue: an Abort carrying
    ///   an ABRT APDU with abort-source dialogue-service-provider
    ///   (3.2.2.1/Q.774).
    ///
    /// `None` in every other case: the originating transaction ID is not
    /// derivable ("discards the message and does not take any other action"),
    /// the message is an End, an Abort or a Unidirectional (Table 7/Q.774:
    /// Discard), or the fault is in a component, which is answered with
    /// [`reject`](Self::reject).
    ///
    /// The Abort is addressed with the received originating transaction ID.
    /// Whether the local transaction named by [`dtid`](Self::dtid) exists and
    /// has to be released is for the caller's transaction state to say.
    pub fn abort(&self) -> Option<Abort> {
        let answerable = matches!(
            self.message_type,
            None | Some(MessageType::Begin | MessageType::Continue)
        );
        if !answerable {
            return None;
        }
        let otid = self.otid.clone()?;
        match self.fault {
            Fault::TransactionPortion { cause } => Some(Abort::p_abort(otid, cause)),
            Fault::DialoguePortion => Some(Abort::u_abort(
                otid,
                Some(DialoguePortion::abnormal_dialogue()),
            )),
            Fault::Component { .. } => None,
        }
    }

    /// The Reject component to send, for a fault in a component.
    ///
    /// 3.2.2.2/Q.774: "Protocol errors in the component portion of a TCAP
    /// message are reported using the Reject component", and "When an invoke
    /// ID is available in a component to be rejected, this ID is reflected in
    /// the Reject component"; otherwise the invoke ID is the NULL.
    ///
    /// `None` when the fault is not in a component, and when the faulty
    /// component is itself a Reject: "the component is discarded and the
    /// local TC-user is advised of the syntax error in the received Reject
    /// component".
    ///
    /// The component sub-layer sends the Reject with the next dialogue
    /// handling primitive of the TC-user (a Continue or an End). A fault in a
    /// component of an End or a Unidirectional leaves no transaction to send
    /// it in; that is the caller's to decide.
    pub fn reject(&self) -> Option<Reject> {
        match self.fault {
            Fault::Component {
                component_type,
                invoke_id,
                problem,
                ..
            } if component_type != Some(ComponentType::Reject) => {
                Some(Reject::general(invoke_id, problem))
            }
            _ => None,
        }
    }
}

impl fmt::Display for DecodeProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.fault, self.detail)
    }
}

impl std::error::Error for DecodeProblem {}

/// The result of decoding one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// Every part of the message was understood, and every octet of the
    /// input belongs to it.
    Complete(TcapMessage),
    /// Some part of the message was not understood.
    Problem(Box<DecodeProblem>),
}

impl Decoded {
    /// The message, if it was fully understood.
    pub fn into_result(self) -> Result<TcapMessage, Box<DecodeProblem>> {
        match self {
            Self::Complete(message) => Ok(message),
            Self::Problem(problem) => Err(problem),
        }
    }

    /// The problem, if there is one.
    pub fn problem(&self) -> Option<&DecodeProblem> {
        match self {
            Self::Complete(_) => None,
            Self::Problem(problem) => Some(problem),
        }
    }
}

/// Decode one TCAP message and report in full what was found.
///
/// `bytes` is the user data of one SCCP message (after reassembly, for
/// XUDT). It has to be exactly one TCAP message: SCCP delivers the user data
/// with its length, so octets after the end of the message are not padding,
/// they mean the "length indicator value does not correspond to length of
/// message" (Table 1/Q.772, badly formatted transaction portion).
pub fn decode_detailed(bytes: &[u8]) -> Decoded {
    match read(bytes) {
        Ok(message) => Decoded::Complete(message),
        Err(problem) => Decoded::Problem(problem),
    }
}

/// What is known about the message while its transaction portion is read.
#[derive(Default, Clone)]
struct Header {
    message_type: Option<MessageType>,
    otid: Option<TransactionId>,
    dtid: Option<TransactionId>,
}

impl Header {
    fn transaction_fault(
        &self,
        cause: PAbortCause,
        detail: impl Into<String>,
    ) -> Box<DecodeProblem> {
        self.fault(Fault::TransactionPortion { cause }, None, detail)
    }

    fn badly_formatted(&self, detail: impl Into<String>) -> Box<DecodeProblem> {
        self.transaction_fault(PAbortCause::BADLY_FORMATTED_TRANSACTION_PORTION, detail)
    }

    fn incorrect(&self, detail: impl Into<String>) -> Box<DecodeProblem> {
        self.transaction_fault(PAbortCause::INCORRECT_TRANSACTION_PORTION, detail)
    }

    fn fault(
        &self,
        fault: Fault,
        partial: Option<TcapMessage>,
        detail: impl Into<String>,
    ) -> Box<DecodeProblem> {
        Box::new(DecodeProblem {
            fault,
            message_type: self.message_type,
            otid: self.otid.clone(),
            dtid: self.dtid.clone(),
            partial,
            detail: detail.into(),
        })
    }
}

fn is_transaction_id(member: &Element<'_>, number: u32) -> bool {
    member.is(APPLICATION, number) && !member.constructed && (1..=4).contains(&member.content.len())
}

/// Recover the transaction IDs from a transaction portion that may be
/// damaged: every element that can be reached is looked at, and the first
/// well-formed originating and destination transaction ID are taken. Q.774
/// leaves it to the implementation whether "a particular abnormality renders
/// a TID not derivable" (Table 7/Q.774).
fn derive_transaction_ids(header: &mut Header, mut content: &[u8]) {
    while let Ok(member) = ber::element(content) {
        if header.otid.is_none() && is_transaction_id(&member, 8) {
            header.otid = Some(member.content.to_vec().into());
        }
        if header.dtid.is_none() && is_transaction_id(&member, 9) {
            header.dtid = Some(member.content.to_vec().into());
        }
        content = member.rest;
    }
}

/// The message type for an identifier octet: application class, constructed,
/// and one of the five tag numbers of Table 8/Q.773.
fn message_type_of(class: u8, constructed: bool, number: u32) -> Option<MessageType> {
    if class == APPLICATION && constructed {
        MessageType::from_tag(number)
    } else {
        None
    }
}

type Members<'a> = std::iter::Peekable<std::vec::IntoIter<Element<'a>>>;

/// Take a mandatory transaction ID off the front of the transaction portion.
fn take_transaction_id(
    header: &Header,
    members: &mut Members<'_>,
    number: u32,
    name: &str,
) -> Result<TransactionId, Box<DecodeProblem>> {
    let Some(member) = members.next_if(|m| m.is(APPLICATION, number)) else {
        return Err(header.incorrect(match members.peek() {
            Some(found) => format!("{name} expected, found {}", found.tag()),
            None => format!("{name} is missing"),
        }));
    };
    if member.constructed {
        // 4.1.1/Q.773: OCTET STRING values are encoded in a primitive form.
        return Err(header.badly_formatted(format!("{name} is constructed")));
    }
    if !(1..=4).contains(&member.content.len()) {
        return Err(header.incorrect(format!(
            "{name} is {} octets long, a transaction ID is 1 to 4 octets",
            member.content.len()
        )));
    }
    Ok(member.content.to_vec().into())
}

/// Take the optional dialogue portion: `[APPLICATION 11]`, constructed,
/// around one `EXTERNAL`.
fn take_dialogue_portion(
    header: &Header,
    members: &mut Members<'_>,
) -> Result<Option<DialoguePortion>, Box<DecodeProblem>> {
    let Some(member) = members.next_if(|m| m.is(APPLICATION, 11)) else {
        return Ok(None);
    };
    if !member.constructed {
        return Err(header.badly_formatted("the dialogue portion is primitive"));
    }
    Ok(Some(DialoguePortion {
        external: Any::new(member.content.to_vec()),
    }))
}

fn nothing_left(header: &Header, members: &mut Members<'_>) -> Result<(), Box<DecodeProblem>> {
    match members.next() {
        None => Ok(()),
        Some(extra) => Err(header.incorrect(format!(
            "{} is not expected at this place in a {} message",
            extra.tag(),
            header
                .message_type
                .map_or("TCAP".to_string(), |t| t.to_string())
        ))),
    }
}

fn read(bytes: &[u8]) -> Result<TcapMessage, Box<DecodeProblem>> {
    let mut header = Header::default();

    // The message is "a single constructor information element" (clause
    // 4/Q.773).
    let outer = match ber::element(bytes) {
        Ok(outer) => outer,
        Err(error) => {
            // The message as a whole cannot be delimited. Take what can be
            // reached past the identifier and length octets.
            if let Some((class, constructed, number, content)) = ber::header(bytes) {
                header.message_type = message_type_of(class, constructed, number);
                derive_transaction_ids(&mut header, content);
            }
            return Err(header.badly_formatted(error));
        }
    };
    header.message_type = message_type_of(outer.class, outer.constructed, outer.number);
    if outer.constructed {
        derive_transaction_ids(&mut header, outer.content);
    }

    if !outer.rest.is_empty() {
        return Err(header.badly_formatted(format!(
            "{} octets follow the end of the message",
            outer.rest.len()
        )));
    }
    let Some(message_type) = header.message_type else {
        return Err(header.transaction_fault(
            PAbortCause::UNRECOGNIZED_MESSAGE_TYPE,
            format!(
                "{}{} is not a message type",
                outer.tag(),
                if outer.constructed { "" } else { " primitive" }
            ),
        ));
    };

    let mut members = match ber::elements(outer.content) {
        Ok(members) => members.into_iter().peekable(),
        Err(error) => return Err(header.badly_formatted(error)),
    };

    // Table 9/Q.773: which transaction IDs each message type carries.
    let otid = match message_type {
        MessageType::Begin | MessageType::Continue => Some(take_transaction_id(
            &header,
            &mut members,
            8,
            "originating transaction ID",
        )?),
        _ => None,
    };
    let dtid =
        match message_type {
            MessageType::End | MessageType::Continue | MessageType::Abort => Some(
                take_transaction_id(&header, &mut members, 9, "destination transaction ID")?,
            ),
            _ => None,
        };
    // From here on the IDs are the ones the grammar gave, and only those.
    header.otid = otid;
    header.dtid = dtid;

    if message_type == MessageType::Abort {
        return read_abort(&header, &mut members);
    }

    let dialogue_portion = take_dialogue_portion(&header, &mut members)?;
    let component_portion = match members.next_if(|m| m.is(APPLICATION, 12)) {
        Some(portion) if !portion.constructed => {
            return Err(header.badly_formatted("the component portion is primitive"));
        }
        Some(portion) if portion.content.is_empty() => {
            // Table 1/Q.772, incorrect transaction portion: "Component
            // Portion Tag present, but no components."
            return Err(header.incorrect("the component portion holds no components"));
        }
        Some(portion) => Some(portion),
        None => None,
    };
    nothing_left(&header, &mut members)?;
    if message_type == MessageType::Unidirectional && component_portion.is_none() {
        return Err(header.incorrect("the component portion is missing"));
    }

    // The transaction portion is sound. The dialogue portion comes next: when
    // it is incorrect, the components are discarded with it.
    if let Some(portion) = &dialogue_portion {
        if let Err(error) = portion.parse() {
            return Err(header.fault(Fault::DialoguePortion, None, error.detail()));
        }
    }

    let components = match component_portion {
        Some(portion) => Some(read_components(&header, &dialogue_portion, &portion)?),
        None => None,
    };
    Ok(assemble(&header, dialogue_portion, components))
}

fn read_abort(
    header: &Header,
    members: &mut Members<'_>,
) -> Result<TcapMessage, Box<DecodeProblem>> {
    let dtid = header.dtid.clone().unwrap_or_default();
    let reason = if let Some(member) = members.next_if(|m| m.is(APPLICATION, 10)) {
        if member.constructed {
            return Err(header.badly_formatted("the P-Abort cause is constructed"));
        }
        let cause = ber::integer(member.content)
            .map_err(|e| header.badly_formatted(format!("P-Abort cause: {e}")))?;
        Some(AbortReason::PAbort(PAbortCause(cause)))
    } else {
        take_dialogue_portion(header, members)?.map(AbortReason::UAbort)
    };
    nothing_left(header, members)?;
    if let Some(AbortReason::UAbort(portion)) = &reason {
        if let Err(error) = portion.parse() {
            return Err(header.fault(Fault::DialoguePortion, None, error.detail()));
        }
    }
    Ok(TcapMessage::Abort(Abort { dtid, reason }))
}

/// Build the message from its parts. `header` holds the transaction IDs the
/// message type calls for.
fn assemble(
    header: &Header,
    dialogue_portion: Option<DialoguePortion>,
    components: Option<Vec<Component>>,
) -> TcapMessage {
    let otid = header.otid.clone().unwrap_or_default();
    let dtid = header.dtid.clone().unwrap_or_default();
    match header.message_type {
        Some(MessageType::Begin) => TcapMessage::Begin(Begin {
            otid,
            dialogue_portion,
            components,
        }),
        Some(MessageType::End) => TcapMessage::End(End {
            dtid,
            dialogue_portion,
            components,
        }),
        Some(MessageType::Continue) => TcapMessage::Continue(Continue {
            otid,
            dtid,
            dialogue_portion,
            components,
        }),
        Some(MessageType::Abort) => TcapMessage::Abort(Abort {
            dtid,
            reason: dialogue_portion.map(AbortReason::UAbort),
        }),
        Some(MessageType::Unidirectional) | None => TcapMessage::Unidirectional(Unidirectional {
            dialogue_portion,
            components: components.unwrap_or_default(),
        }),
    }
}

/// Decode one component, and prove that nothing of it was dropped: the
/// decoded value is encoded again and the two encodings are compared element
/// by element.
fn read_component(wire: &[u8]) -> Result<Component, Mismatch> {
    let unreadable = |detail: String| Mismatch::Structure(detail);
    let component = rasn::ber::decode::<Component>(wire).map_err(|e| unreadable(e.to_string()))?;
    let canonical = rasn::ber::encode(&component).map_err(|e| unreadable(e.to_string()))?;
    ber::nothing_dropped(wire, &canonical)?;
    Ok(component)
}

/// The invoke ID of a component that could not be decoded, when it can be
/// read all the same: every component type starts with it, as a one-octet
/// INTEGER (`InvokeIdType ::= INTEGER (-128..127)`).
fn derive_invoke_id(content: &[u8]) -> Option<InvokeId> {
    let first = ber::element(content).ok()?;
    match first.content {
        &[octet] if first.is(UNIVERSAL, 2) && !first.constructed => Some(octet as InvokeId),
        _ => None,
    }
}

/// The component type for an identifier: context class, constructed, and one
/// of the five tag numbers of Table 19/Q.773.
fn component_type_of(class: u8, constructed: bool, number: u32) -> Option<ComponentType> {
    if class == CONTEXT && constructed {
        ComponentType::from_tag(number)
    } else {
        None
    }
}

/// Tell a component whose encoding is broken (badly structured, 3.7.1.3/Q.772)
/// from one whose elements are not those of its type (mistyped,
/// 3.7.1.2/Q.772).
fn diagnose_component(
    component: &Element<'_>,
    component_type: ComponentType,
    mismatch: &Mismatch,
) -> GeneralProblem {
    if matches!(mismatch, Mismatch::Encoding(_)) {
        return GeneralProblem::BADLY_STRUCTURED_COMPONENT;
    }
    let Ok(members) = ber::elements(component.content) else {
        return GeneralProblem::BADLY_STRUCTURED_COMPONENT;
    };
    // The result of a ReturnResult is a SEQUENCE of the component's own
    // elements. An operation argument is the TC-user's to judge, so a
    // parameter is not looked into.
    let is_result = matches!(
        component_type,
        ComponentType::ReturnResultLast | ComponentType::ReturnResultNotLast
    );
    let broken_result = is_result
        && members.iter().any(|member| {
            member.is(UNIVERSAL, 16) && member.constructed && ber::elements(member.content).is_err()
        });
    if broken_result {
        GeneralProblem::BADLY_STRUCTURED_COMPONENT
    } else {
        GeneralProblem::MISTYPED_COMPONENT
    }
}

fn read_components(
    header: &Header,
    dialogue_portion: &Option<DialoguePortion>,
    portion: &Element<'_>,
) -> Result<Vec<Component>, Box<DecodeProblem>> {
    let mut components = Vec::new();
    let mut rest = portion.content;
    while !rest.is_empty() {
        let index = components.len();
        let fault = |component_type, invoke_id, problem, detail: String, read: &[Component]| {
            let partial = assemble(
                header,
                dialogue_portion.clone(),
                (!read.is_empty()).then(|| read.to_vec()),
            );
            header.fault(
                Fault::Component {
                    index,
                    component_type,
                    invoke_id,
                    problem,
                },
                Some(partial),
                detail,
            )
        };

        let component = match ber::element(rest) {
            Ok(component) => component,
            Err(error) => {
                // What is left of the component portion does not hold an
                // element: the encoding rules are violated. The identifier
                // may still say which component it was meant to be.
                let (component_type, invoke_id) = match ber::header(rest) {
                    Some((class, constructed, number, content)) => {
                        let component_type = component_type_of(class, constructed, number);
                        (
                            component_type,
                            component_type.and_then(|_| derive_invoke_id(content)),
                        )
                    }
                    None => (None, None),
                };
                return Err(fault(
                    component_type,
                    invoke_id,
                    GeneralProblem::BADLY_STRUCTURED_COMPONENT,
                    error,
                    &components,
                ));
            }
        };
        rest = component.rest;

        let Some(component_type) =
            component_type_of(component.class, component.constructed, component.number)
        else {
            return Err(fault(
                None,
                None,
                GeneralProblem::UNRECOGNIZED_COMPONENT,
                format!("{} is not a component type", component.tag()),
                &components,
            ));
        };
        match read_component(component.whole) {
            Ok(component) => components.push(component),
            Err(mismatch) => {
                return Err(fault(
                    Some(component_type),
                    derive_invoke_id(component.content),
                    diagnose_component(&component, component_type, &mismatch),
                    format!("{component_type}: {}", mismatch.detail()),
                    &components,
                ));
            }
        }
    }
    Ok(components)
}
