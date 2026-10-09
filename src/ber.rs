//! A small BER (X.690) element reader and writer, and the guard against data
//! that disappears during decoding.
//!
//! # Why the crate does not simply call `rasn::ber::decode`
//!
//! `rasn` 0.28 (checked on 0.28.14 and 0.28.15) is lenient in three places,
//! and in each of them a decode succeeds with less than was on the wire:
//!
//! * A SEQUENCE OF stops at the first element that fails to decode and returns
//!   the elements before it (`decode_sequence_of`: `Err(_) => break`). When
//!   the failed element is the last one, nothing is left over to raise
//!   "unexpected extra data", and the list comes back one element short, or
//!   empty. The component portion is a SEQUENCE OF Component.
//! * An OPTIONAL member whose content does not decode can be reported as
//!   absent after its octets have been consumed. The `result` of a
//!   ReturnResult is such a member.
//! * Octets after the end of the outermost value are ignored.
//!
//! For TCAP that means an Invoke or a ReturnResult the peer sent never reaches
//! the TC-user, no Reject goes back, and the peer runs into its invoke timer.
//!
//! [`nothing_dropped`] is the structural guard: a decoded value is encoded
//! again and the two encodings are walked side by side. Every element this
//! crate would emit must be met by an element with the same tag on the wire,
//! and the wire must hold nothing more. The form has to match as well: an
//! element this crate emits as constructed must be constructed on the wire
//! (X.690 8.9.1, 8.10.1, 8.14.2), and one it emits as primitive must be
//! primitive (an INTEGER, a NULL and an OBJECT IDENTIFIER always are, and
//! 4.1.1/Q.773 has OCTET STRING and BIT STRING values primitive too). `rasn`
//! checks neither on an implicit tag, and reads the content octets of a
//! constructed `[0]` as if they were the INTEGER. Contents of primitive
//! elements are not compared, since BER lets a sender write the same value in
//! more than one way. An operation argument is an open type that the crate
//! carries verbatim: it has the form the sender gave it, and where its inside
//! is not well-formed BER, the octets themselves are compared.

/// Nesting allowed while reading. A TCAP message nests about six levels above
/// the operation argument; the bound keeps a hostile argument from exhausting
/// the stack.
pub(crate) const MAX_DEPTH: usize = 64;

/// Tag class UNIVERSAL.
pub(crate) const UNIVERSAL: u8 = 0;
/// Tag class APPLICATION.
pub(crate) const APPLICATION: u8 = 1;
/// Tag class context-specific.
pub(crate) const CONTEXT: u8 = 2;

/// One tag-length-value, borrowed from the input.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Element<'a> {
    pub(crate) class: u8,
    pub(crate) number: u32,
    pub(crate) constructed: bool,
    /// The content octets, without the end-of-contents of the indefinite form.
    pub(crate) content: &'a [u8],
    /// The whole element: identifier, length, content and end-of-contents.
    pub(crate) whole: &'a [u8],
    /// What follows the element.
    pub(crate) rest: &'a [u8],
}

impl Element<'_> {
    /// The tag in ASN.1 notation, for messages.
    pub(crate) fn tag(&self) -> String {
        let class = match self.class {
            UNIVERSAL => "UNIVERSAL ",
            APPLICATION => "APPLICATION ",
            CONTEXT => "",
            _ => "PRIVATE ",
        };
        format!("[{class}{}]", self.number)
    }

    /// Whether the element carries this class and tag number.
    pub(crate) fn is(&self, class: u8, number: u32) -> bool {
        self.class == class && self.number == number
    }
}

/// Split one tag-length-value off the front of `input`.
pub(crate) fn element(input: &[u8]) -> Result<Element<'_>, String> {
    element_at(input, 0)
}

fn element_at(input: &[u8], depth: usize) -> Result<Element<'_>, String> {
    if depth > MAX_DEPTH {
        return Err(format!("nesting deeper than {MAX_DEPTH} levels"));
    }
    let (&first, mut rest) = input
        .split_first()
        .ok_or_else(|| "truncated: no identifier octet".to_string())?;
    let class = first >> 6;
    let constructed = first & 0x20 != 0;
    let mut number = u32::from(first & 0x1f);
    if number == 0x1f {
        // High tag number form: base 128, most significant group first.
        number = 0;
        loop {
            let (&octet, tail) = rest
                .split_first()
                .ok_or_else(|| "truncated tag number".to_string())?;
            rest = tail;
            number = number
                .checked_mul(128)
                .and_then(|n| n.checked_add(u32::from(octet & 0x7f)))
                .ok_or_else(|| "tag number too large".to_string())?;
            if octet & 0x80 == 0 {
                break;
            }
        }
    }

    let (&length_octet, after_length) = rest
        .split_first()
        .ok_or_else(|| "truncated: no length octet".to_string())?;
    if length_octet == 0x80 {
        // Indefinite form (X.690 8.1.3.6): only a constructed element may use
        // it, and the content runs to the matching end-of-contents.
        if !constructed {
            return Err("indefinite length on a primitive element".to_string());
        }
        let mut cursor = after_length;
        loop {
            if let Some(after) = cursor.strip_prefix(&[0x00, 0x00]) {
                return Ok(Element {
                    class,
                    number,
                    constructed,
                    content: &after_length[..after_length.len() - cursor.len()],
                    whole: &input[..input.len() - after.len()],
                    rest: after,
                });
            }
            cursor = element_at(cursor, depth + 1)
                .map_err(|e| format!("inside an indefinite-length element: {e}"))?
                .rest;
        }
    }
    let (length, after_length) = if length_octet < 0x80 {
        (usize::from(length_octet), after_length)
    } else {
        let count = usize::from(length_octet & 0x7f);
        if count > std::mem::size_of::<usize>() || after_length.len() < count {
            return Err("unusable length field".to_string());
        }
        let (octets, tail) = after_length.split_at(count);
        let length = octets
            .iter()
            .fold(0usize, |value, &octet| (value << 8) | usize::from(octet));
        (length, tail)
    };
    if after_length.len() < length {
        return Err(format!(
            "length {length} runs past the {} octets that are left",
            after_length.len()
        ));
    }
    let (content, rest) = after_length.split_at(length);
    Ok(Element {
        class,
        number,
        constructed,
        content,
        whole: &input[..input.len() - rest.len()],
        rest,
    })
}

/// The identifier of the element at the front of `input` and everything
/// after its length field, whatever the length says. For a message whose
/// stated length cannot be trusted.
pub(crate) fn header(input: &[u8]) -> Option<(u8, bool, u32, &[u8])> {
    let (&first, mut rest) = input.split_first()?;
    let mut number = u32::from(first & 0x1f);
    if number == 0x1f {
        number = 0;
        loop {
            let (&octet, tail) = rest.split_first()?;
            rest = tail;
            number = number
                .checked_mul(128)?
                .checked_add(u32::from(octet & 0x7f))?;
            if octet & 0x80 == 0 {
                break;
            }
        }
    }
    let (&length_octet, rest) = rest.split_first()?;
    let skip = if length_octet > 0x80 {
        usize::from(length_octet & 0x7f)
    } else {
        0
    };
    Some((first >> 6, first & 0x20 != 0, number, rest.get(skip..)?))
}

/// Split `content` into the elements it holds. Fails on the first octet that
/// does not start a well-formed element.
pub(crate) fn elements(mut content: &[u8]) -> Result<Vec<Element<'_>>, String> {
    let mut out = Vec::new();
    while !content.is_empty() {
        let next = element(content)?;
        content = next.rest;
        out.push(next);
    }
    Ok(out)
}

/// The value of INTEGER content octets (X.690 8.3), when it fits an `i64`.
pub(crate) fn integer(content: &[u8]) -> Result<i64, String> {
    let (&first, _) = content
        .split_first()
        .ok_or_else(|| "INTEGER with no content octets".to_string())?;
    if content.len() > 8 {
        return Err(format!("INTEGER of {} octets", content.len()));
    }
    let seed: i64 = if first & 0x80 != 0 { -1 } else { 0 };
    Ok(content
        .iter()
        .fold(seed, |value, &octet| (value << 8) | i64::from(octet)))
}

/// INTEGER content octets: two's complement, fewest octets (X.690 8.3.2).
pub(crate) fn integer_content(value: i64) -> Vec<u8> {
    let octets = value.to_be_bytes();
    let mut start = 0;
    while start < octets.len() - 1 {
        let redundant = (octets[start] == 0x00 && octets[start + 1] & 0x80 == 0)
            || (octets[start] == 0xff && octets[start + 1] & 0x80 != 0);
        if !redundant {
            break;
        }
        start += 1;
    }
    octets[start..].to_vec()
}

/// One element with a low tag number (below 31) and a definite length in the
/// fewest octets, as 4.1.1/Q.773 requires of a sender.
pub(crate) fn tlv(identifier: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + 6);
    out.push(identifier);
    if content.len() < 0x80 {
        out.push(content.len() as u8);
    } else {
        let octets = content.len().to_be_bytes();
        let skip = octets.iter().take_while(|&&octet| octet == 0).count();
        out.push(0x80 | (octets.len() - skip) as u8);
        out.extend_from_slice(&octets[skip..]);
    }
    out.extend_from_slice(content);
    out
}

/// Why a decoded value does not account for what was on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mismatch {
    /// An element is there and was not taken up by the decoded value, or
    /// something else in the structure differs.
    Structure(String),
    /// An element has the wrong form (primitive where constructed is
    /// required or the reverse), or cannot be delimited: the encoding rules
    /// are violated.
    Encoding(String),
}

impl Mismatch {
    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::Structure(detail) | Self::Encoding(detail) => detail,
        }
    }
}

fn walk(mut wire: &[u8], mut canonical: &[u8], depth: usize) -> Result<(), Mismatch> {
    if depth > MAX_DEPTH {
        return Err(Mismatch::Encoding(format!(
            "nesting deeper than {MAX_DEPTH} levels"
        )));
    }
    while !canonical.is_empty() {
        let Ok(expected) = element(canonical) else {
            // The crate's own encoding does not hold an element here. Only an
            // open-type value (an operation argument), which is carried and
            // emitted verbatim, can be malformed inside; whether it is, is for
            // the TC-user to say. It is accounted for when the same octets
            // are on the wire.
            return if wire == canonical {
                Ok(())
            } else {
                Err(Mismatch::Structure(
                    "an open-type value differs from what was received".to_string(),
                ))
            };
        };
        if wire.is_empty() {
            // Cannot happen for a value that was just decoded from `wire`.
            return Err(Mismatch::Structure(format!(
                "{} is missing on the wire",
                expected.tag()
            )));
        }
        let found = element(wire).map_err(Mismatch::Encoding)?;
        if (found.class, found.number) != (expected.class, expected.number) {
            return Err(Mismatch::Structure(format!(
                "element {} is present but its content could not be decoded",
                found.tag()
            )));
        }
        if expected.constructed != found.constructed {
            return Err(Mismatch::Encoding(format!(
                "element {} is {} on the wire, it has to be {}",
                found.tag(),
                if found.constructed {
                    "constructed"
                } else {
                    "primitive"
                },
                if expected.constructed {
                    "constructed"
                } else {
                    "primitive"
                },
            )));
        }
        if expected.constructed {
            walk(found.content, expected.content, depth + 1)?;
        }
        wire = found.rest;
        canonical = expected.rest;
    }
    if wire.is_empty() {
        return Ok(());
    }
    match element(wire) {
        Ok(found) => Err(Mismatch::Structure(format!(
            "element {} is present but its content could not be decoded",
            found.tag()
        ))),
        Err(_) => Err(Mismatch::Encoding(format!(
            "{} octets that are not an element",
            wire.len()
        ))),
    }
}

/// Check that `canonical`, the encoding of the value decoded from `wire`,
/// accounts for every element of `wire`.
pub(crate) fn nothing_dropped(wire: &[u8], canonical: &[u8]) -> Result<(), Mismatch> {
    walk(wire, canonical, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_long_and_indefinite_lengths_are_read() {
        let short = element(&[0x30, 0x03, 0x02, 0x01, 0x07, 0xff]).unwrap();
        assert_eq!(short.content, &[0x02, 0x01, 0x07]);
        assert_eq!(short.whole, &[0x30, 0x03, 0x02, 0x01, 0x07]);
        assert_eq!(short.rest, &[0xff]);

        let long = element(&[0x30, 0x81, 0x03, 0x02, 0x01, 0x07]).unwrap();
        assert_eq!(long.content, &[0x02, 0x01, 0x07]);

        let indefinite = element(&[0x30, 0x80, 0x02, 0x01, 0x07, 0x00, 0x00, 0xff]).unwrap();
        assert_eq!(indefinite.content, &[0x02, 0x01, 0x07]);
        assert_eq!(indefinite.whole.len(), 7);
        assert_eq!(indefinite.rest, &[0xff]);
    }

    #[test]
    fn broken_framing_is_an_error() {
        assert!(element(&[]).is_err());
        assert!(element(&[0x30]).is_err());
        assert!(element(&[0x30, 0x05, 0x02, 0x01]).is_err());
        assert!(element(&[0x30, 0x82, 0x01]).is_err());
        assert!(element(&[0x30, 0x80, 0x02, 0x01, 0x07]).is_err());
        // Indefinite length is for constructed elements only.
        assert!(element(&[0x04, 0x80, 0x00, 0x00]).is_err());
    }

    #[test]
    fn high_tag_numbers_are_read() {
        // [30] constructed is BE, [52] primitive is 9F 34.
        assert!(element(&[0xbe, 0x00]).unwrap().is(CONTEXT, 30));
        assert!(element(&[0x9f, 0x34, 0x00]).unwrap().is(CONTEXT, 52));
    }

    #[test]
    fn integers_are_read_and_written_in_the_fewest_octets() {
        for (value, octets) in [
            (0i64, vec![0x00]),
            (1, vec![0x01]),
            (127, vec![0x7f]),
            (128, vec![0x00, 0x80]),
            (-1, vec![0xff]),
            (-128, vec![0x80]),
            (-129, vec![0xff, 0x7f]),
            (300, vec![0x01, 0x2c]),
        ] {
            assert_eq!(integer_content(value), octets, "{value}");
            assert_eq!(integer(&octets), Ok(value), "{value}");
        }
        assert!(integer(&[]).is_err());
        assert!(integer(&[0; 9]).is_err());
    }

    #[test]
    fn lengths_are_written_in_the_fewest_octets() {
        assert_eq!(tlv(0x04, &[0xaa]), vec![0x04, 0x01, 0xaa]);
        assert_eq!(tlv(0x04, &[0; 127])[..2], [0x04, 0x7f]);
        assert_eq!(tlv(0x04, &[0; 128])[..3], [0x04, 0x81, 0x80]);
        assert_eq!(tlv(0x04, &[0; 256])[..4], [0x04, 0x82, 0x01, 0x00]);
    }

    #[test]
    fn identical_encodings_pass() {
        let bytes = [0x30, 0x08, 0x80, 0x01, 0x07, 0xa3, 0x03, 0x81, 0x01, 0x02];
        assert_eq!(nothing_dropped(&bytes, &bytes), Ok(()));
    }

    #[test]
    fn a_dropped_element_is_reported_with_its_tag() {
        // [3] is on the wire, the re-encoding has only [0].
        let wire = [0x30, 0x08, 0x80, 0x01, 0x07, 0xa3, 0x03, 0x80, 0x01, 0x02];
        let canonical = [0x30, 0x03, 0x80, 0x01, 0x07];
        let error = nothing_dropped(&wire, &canonical).unwrap_err();
        assert!(
            matches!(&error, Mismatch::Structure(d) if d.contains("[3]")),
            "{error:?}"
        );
    }

    #[test]
    fn an_element_dropped_before_another_is_reported() {
        // [2] vanished, [4] survived: the walk meets [2] where it expects [4].
        let wire = [0x30, 0x06, 0x82, 0x01, 0x02, 0x84, 0x01, 0x00];
        let canonical = [0x30, 0x03, 0x84, 0x01, 0x00];
        let error = nothing_dropped(&wire, &canonical).unwrap_err();
        assert!(
            matches!(&error, Mismatch::Structure(d) if d.contains("[2]")),
            "{error:?}"
        );
    }

    #[test]
    fn length_forms_do_not_matter() {
        let canonical = [0x30, 0x05, 0xa2, 0x03, 0x80, 0x01, 0x02];
        let long = [0x30, 0x81, 0x06, 0xa2, 0x81, 0x03, 0x80, 0x01, 0x02];
        let indefinite = [
            0x30, 0x80, 0xa2, 0x80, 0x80, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(nothing_dropped(&long, &canonical), Ok(()));
        assert_eq!(nothing_dropped(&indefinite, &canonical), Ok(()));
    }

    #[test]
    fn primitive_content_is_not_compared() {
        assert_eq!(
            nothing_dropped(
                &[0x30, 0x03, 0x81, 0x01, 0x01],
                &[0x30, 0x03, 0x81, 0x01, 0xff]
            ),
            Ok(())
        );
    }

    #[test]
    fn a_primitive_element_where_a_constructed_one_is_required_is_reported() {
        let canonical = [0x30, 0x07, 0xa0, 0x05, 0xa1, 0x03, 0x80, 0x01, 0x07];
        let wire = [0x30, 0x07, 0x80, 0x05, 0xa1, 0x03, 0x80, 0x01, 0x07];
        let error = nothing_dropped(&wire, &canonical).unwrap_err();
        assert!(
            matches!(&error, Mismatch::Encoding(d) if d.contains("[0]") && d.contains("constructed")),
            "{error:?}"
        );
    }

    #[test]
    fn a_constructed_element_where_a_primitive_one_is_required_is_reported() {
        // [0] IMPLICIT INTEGER 1 is 80 01 01. rasn reads A0 03 02 01 01 as the
        // INTEGER 0x020101; the re-encoding is primitive and the wire is not.
        let canonical = [0x30, 0x05, 0x80, 0x03, 0x02, 0x01, 0x01];
        let wire = [0x30, 0x05, 0xa0, 0x03, 0x02, 0x01, 0x01];
        let error = nothing_dropped(&wire, &canonical).unwrap_err();
        assert!(
            matches!(&error, Mismatch::Encoding(d) if d.contains("[0]") && d.contains("primitive")),
            "{error:?}"
        );
    }

    #[test]
    fn a_verbatim_value_that_is_malformed_inside_is_compared_octet_by_octet() {
        // A SEQUENCE whose content is an INTEGER announcing five octets with
        // one present, after a sound INTEGER.
        let value = [0x30, 0x06, 0x02, 0x01, 0x07, 0x02, 0x05, 0x3a];
        assert_eq!(nothing_dropped(&value, &value), Ok(()));
        let other = [0x30, 0x06, 0x02, 0x01, 0x07, 0x02, 0x05, 0x3b];
        assert!(nothing_dropped(&other, &value).is_err());
    }

    #[test]
    fn trailing_octets_after_the_value_are_reported() {
        let error = nothing_dropped(&[0x04, 0x01, 0x15, 0xff], &[0x04, 0x01, 0x15]).unwrap_err();
        assert!(matches!(error, Mismatch::Encoding(_)), "{error:?}");
    }

    #[test]
    fn nesting_is_bounded() {
        let mut wire = Vec::new();
        for _ in 0..100 {
            wire.extend_from_slice(&[0x30, 0x80]);
        }
        for _ in 0..100 {
            wire.extend_from_slice(&[0x00, 0x00]);
        }
        assert!(element(&wire).is_err());
        // Not readable as elements, so it is compared as octets.
        assert_eq!(nothing_dropped(&wire, &wire), Ok(()));
        assert!(nothing_dropped(&wire[2..], &wire).is_err());
    }
}
