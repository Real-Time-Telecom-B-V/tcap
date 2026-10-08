use std::fmt;

use rasn::prelude::*;

/// TCAP Transaction ID: an OCTET STRING of 1 to 4 octets (3.1/Q.773,
/// `OrigTransactionID` and `DestTransactionID`, `SIZE (1..4)`).
///
/// The size is checked by [`crate::encode`] and by the decoder; a transaction
/// ID of any other length is an incorrect transaction portion.
pub type TransactionId = rasn::types::OctetString;

/// Invoke ID: `InvokeIdType ::= INTEGER (-128..127)` (3.1/Q.773), one content
/// octet on the wire.
pub type InvokeId = i8;

/// Operation Code — local or global.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum OperationCode {
    Local(i64),
    Global(rasn::types::ObjectIdentifier),
}

impl fmt::Display for OperationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(v) => write!(f, "local({v})"),
            Self::Global(oid) => write!(f, "global({oid})"),
        }
    }
}

/// Error Code — local or global.
#[derive(Debug, Clone, PartialEq, Eq, rasn::AsnType, rasn::Decode, rasn::Encode)]
#[rasn(choice)]
pub enum ErrorCode {
    Local(i64),
    Global(rasn::types::ObjectIdentifier),
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(v) => write!(f, "local({v})"),
            Self::Global(oid) => write!(f, "global({oid})"),
        }
    }
}

/// Declares an INTEGER with named numbers as a newtype with one associated
/// constant per name. Q.773 gives these types as plain `INTEGER { name (n) }`,
/// so a value without a name is still a valid value of the type: it decodes,
/// and prints as its number.
macro_rules! named_integer {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$cmeta:meta])* $constant:ident = $value:literal => $label:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rasn::AsnType, rasn::Decode, rasn::Encode)]
        #[rasn(delegate)]
        pub struct $name(pub i64);

        impl $name {
            $( $(#[$cmeta])* pub const $constant: Self = Self($value); )+

            /// The INTEGER value on the wire.
            pub fn value(self) -> i64 {
                self.0
            }

            /// The name Q.773 gives this value, if it has one.
            pub fn name(self) -> Option<&'static str> {
                match self.0 {
                    $( $value => Some($label), )+
                    _ => None,
                }
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.name() {
                    Some(name) => write!(f, "{name}({})", self.0),
                    None => write!(f, "{}", self.0),
                }
            }
        }
    };
}

named_integer! {
    /// `P-AbortCause ::= [APPLICATION 10] IMPLICIT INTEGER` (3.1/Q.773, values
    /// in Table 12/Q.773, meanings in 2.3/Q.772): why the transaction
    /// sub-layer aborted a transaction.
    PAbortCause {
        /// The message type is not one of those defined (2.3.1/Q.772).
        UNRECOGNIZED_MESSAGE_TYPE = 0 => "unrecognizedMessageType",
        /// A transaction ID was received for which no transaction exists at
        /// the receiving node (2.3.2/Q.772).
        UNRECOGNIZED_TRANSACTION_ID = 1 => "unrecognizedTransactionID",
        /// The transaction portion does not conform to the encoding rules of
        /// 4.1/Q.773 (2.3.3/Q.772).
        BADLY_FORMATTED_TRANSACTION_PORTION = 2 => "badlyFormattedTransactionPortion",
        /// The elemental structure of the transaction portion does not conform
        /// to 3.1/Q.773 (2.3.4/Q.772).
        INCORRECT_TRANSACTION_PORTION = 3 => "incorrectTransactionPortion",
        /// Resources are not available to start a transaction (2.3.5/Q.772).
        RESOURCE_LIMITATION = 4 => "resourceLimitation",
    }
}

named_integer! {
    /// `GeneralProblem` (3.1/Q.773, Table 26/Q.773, 3.7.1/Q.772): problems the
    /// component sub-layer finds in any component type.
    GeneralProblem {
        /// The component type is not one of those defined.
        UNRECOGNIZED_COMPONENT = 0 => "unrecognizedComponent",
        /// The elemental structure of the component does not conform to
        /// 3.1/Q.773.
        MISTYPED_COMPONENT = 1 => "mistypedComponent",
        /// The contents of the component do not conform to the encoding rules
        /// of 4.1/Q.773.
        BADLY_STRUCTURED_COMPONENT = 2 => "badlyStructuredComponent",
    }
}

named_integer! {
    /// `InvokeProblem` (3.1/Q.773, Table 27/Q.773, 3.7.2/Q.772).
    InvokeProblem {
        DUPLICATE_INVOKE_ID = 0 => "duplicateInvokeID",
        UNRECOGNIZED_OPERATION = 1 => "unrecognizedOperation",
        MISTYPED_PARAMETER = 2 => "mistypedParameter",
        RESOURCE_LIMITATION = 3 => "resourceLimitation",
        INITIATING_RELEASE = 4 => "initiatingRelease",
        UNRECOGNIZED_LINKED_ID = 5 => "unrecognizedLinkedID",
        LINKED_RESPONSE_UNEXPECTED = 6 => "linkedResponseUnexpected",
        UNEXPECTED_LINKED_OPERATION = 7 => "unexpectedLinkedOperation",
    }
}

named_integer! {
    /// `ReturnResultProblem` (3.1/Q.773, Table 28/Q.773, 3.7.3/Q.772).
    ReturnResultProblem {
        UNRECOGNIZED_INVOKE_ID = 0 => "unrecognizedInvokeID",
        RETURN_RESULT_UNEXPECTED = 1 => "returnResultUnexpected",
        MISTYPED_PARAMETER = 2 => "mistypedParameter",
    }
}

named_integer! {
    /// `ReturnErrorProblem` (3.1/Q.773, Table 29/Q.773, 3.7.4/Q.772).
    ReturnErrorProblem {
        UNRECOGNIZED_INVOKE_ID = 0 => "unrecognizedInvokeID",
        RETURN_ERROR_UNEXPECTED = 1 => "returnErrorUnexpected",
        UNRECOGNIZED_ERROR = 2 => "unrecognizedError",
        UNEXPECTED_ERROR = 3 => "unexpectedError",
        MISTYPED_PARAMETER = 4 => "mistypedParameter",
    }
}
