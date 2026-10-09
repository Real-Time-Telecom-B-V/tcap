//! Independent-decoder harness: hand a TCAP message to Wireshark's TCAP
//! dissector and read back the fields it decoded.
//!
//! A round-trip through this crate's own decoder cannot catch a mistake that
//! the encoder and the decoder share (a wrong tag number, a missing explicit
//! wrapper). `tshark` dissects TCAP from the ASN.1 of Q.773 with a different
//! toolchain, so it does not share those mistakes.
//!
//! The message is wrapped the way it travels on a signalling link:
//!
//! ```text
//!   TCAP message
//!     -> SCCP UDT (Q.713), called and calling party addressed by subsystem
//!        number 8
//!     -> M3UA DATA (RFC 4666), service indicator 3
//!     -> text2pcap dummy SCTP/IP/Ethernet (payload protocol identifier 3)
//! ```
//!
//! and dissected with `tshark -T pdml`, which yields every decoded field with
//! its abbreviation, its display value and its raw octets. Tests assert on
//! those fields. A dissection that merely does not crash proves nothing.
//!
//! `tshark` and `text2pcap` are optional. When either is missing the helpers
//! return `None` after printing a `SKIP` line, and the hand-derived byte
//! vectors in the same tests still pin the encoding. Set
//! `TCAP_REQUIRE_TSHARK=1` to turn a missing tool into a test failure.
//!
//! All addresses are synthetic: point codes 1 and 2, no global titles.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use tcap::{DecodeProblem, Decoded, TcapMessage};

/// One field of the dissection, as `tshark -T pdml` reports it.
#[derive(Debug, Clone)]
pub struct Field {
    /// Field abbreviation, for example `tcap.invokeID`.
    pub name: String,
    /// Display value, for example `1`.
    pub show: String,
    /// The labelled line Wireshark prints, for example `invokeID: 1`.
    pub showname: String,
    /// Raw octets covered by the field, lowercase hex. For a primitive BER
    /// type these are the content octets.
    pub value: String,
}

/// A dissected frame.
#[derive(Debug, Clone)]
pub struct Dissection {
    pub fields: Vec<Field>,
    pub pdml: String,
}

impl Dissection {
    /// Raw octets (hex) of every occurrence of `name`, in frame order.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.name == name)
            .map(|f| f.value.as_str())
            .collect()
    }

    /// Display value of every occurrence of `name`, in frame order.
    pub fn shows(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.name == name)
            .map(|f| f.show.as_str())
            .collect()
    }

    /// Assert `name` was dissected exactly once with these raw octets.
    #[track_caller]
    pub fn hex(&self, name: &str, expected: &str) -> &Self {
        self.hex_all(name, &[expected])
    }

    /// Assert `name` was dissected with these raw octets, once per entry.
    #[track_caller]
    pub fn hex_all(&self, name: &str, expected: &[&str]) -> &Self {
        assert_eq!(
            self.values(name),
            expected,
            "raw octets of {name}\n{}",
            self.summary()
        );
        self
    }

    /// Assert `name` was dissected exactly once with this display value.
    #[track_caller]
    pub fn show(&self, name: &str, expected: &str) -> &Self {
        self.show_all(name, &[expected])
    }

    /// Assert `name` was dissected with these display values, once per entry.
    #[track_caller]
    pub fn show_all(&self, name: &str, expected: &[&str]) -> &Self {
        assert_eq!(
            self.shows(name),
            expected,
            "display value of {name}\n{}",
            self.summary()
        );
        self
    }

    /// Assert `name` is present exactly once (for constructed members whose
    /// content is asserted through their children).
    #[track_caller]
    pub fn present(&self, name: &str) -> &Self {
        assert_eq!(
            self.values(name).len(),
            1,
            "{name} should be dissected exactly once\n{}",
            self.summary()
        );
        self
    }

    /// Assert `name` does not occur.
    #[track_caller]
    pub fn absent(&self, name: &str) -> &Self {
        assert!(
            self.values(name).is_empty(),
            "{name} should not be dissected\n{}",
            self.summary()
        );
        self
    }

    /// The reasons this dissection is not clean, empty when it is.
    ///
    /// Only TCAP and what it carries is judged. The layers below are
    /// scaffolding built here.
    pub fn problems(&self) -> Vec<String> {
        let upper = self
            .fields
            .iter()
            .position(|f| f.name == "tcap")
            .unwrap_or(0);
        let mut out = Vec::new();
        for f in &self.fields[upper..] {
            let name = f.name.as_str();
            let bad_name = name.starts_with("_ws.malformed")
                || name.starts_with("_ws.expert")
                || name.starts_with("_ws.unreassembled")
                || name.starts_with("ber.error")
                || name.starts_with("ber.unknown");
            // A capital "Unknown" at the start of a label is Wireshark not
            // recognising something; in lower case it is part of a name
            // (unknownSubscriber).
            let label = f.showname.to_ascii_lowercase();
            let bad_label = label.contains("malformed")
                || f.showname.starts_with("Unknown")
                || label.contains("ber error")
                || label.contains("expert info");
            if bad_name || bad_label {
                out.push(format!("{name}: {}", f.showname));
            }
        }
        out
    }

    /// Assert Wireshark dissected TCAP and flagged nothing.
    #[track_caller]
    pub fn clean(&self) -> &Self {
        assert!(
            self.fields.iter().any(|f| f.name == "tcap"),
            "Wireshark did not hand the frame to its TCAP dissector\n{}",
            self.summary()
        );
        let problems = self.problems();
        assert!(
            problems.is_empty(),
            "Wireshark rejects this encoding:\n  {}\n{}",
            problems.join("\n  "),
            self.summary()
        );
        self
    }

    /// The upper-layer fields, one per line, for assertion messages.
    pub fn summary(&self) -> String {
        self.fields
            .iter()
            .filter(|f| {
                ["tcap", "gsm_old.", "gsm_map.", "ber.", "_ws."]
                    .iter()
                    .any(|p| f.name.starts_with(p))
            })
            .map(|f| format!("  {} = {} [{}]", f.name, f.show, f.value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn tool(name: &str) -> bool {
    Command::new(name)
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether `tshark` and `text2pcap` can be run. Prints a `SKIP` line when not.
pub fn tshark_available() -> bool {
    if tool("tshark") && tool("text2pcap") {
        return true;
    }
    let message = "tshark / text2pcap not found: the independent Wireshark dissection \
                   is skipped, only the byte-level vectors run";
    assert!(
        std::env::var_os("TCAP_REQUIRE_TSHARK").is_none(),
        "TCAP_REQUIRE_TSHARK is set but {message}"
    );
    eprintln!("SKIP: {message}");
    false
}

/// The subsystem number both SCCP addresses use. Wireshark's TCAP dissector
/// reads the transaction portion and the dialogue portion itself (`tcap.*`
/// fields) and hands each component to the application protocol registered
/// for the subsystem number. For 8 that is its MAP dissector, which reads
/// the component with its own copy of the component grammar (`gsm_old.*`
/// fields: `gsm_old.invokeID`, `gsm_old.localValue`, `gsm_old.generalProblem`
/// and so on) before it looks at the operation argument. The tests use MAP
/// operation 58 with its plain octet string argument and result so that the
/// argument dissects too.
const SUBSYSTEM: u8 = 8;

/// SCCP UDT (Q.713 clause 4.10) inside an M3UA DATA message.
///
/// ```text
/// 09           message type: unitdata
/// 00           protocol class 0, no special options
/// 03 05 07     pointers to called address, calling address, data
/// 02 42 08     called:  length 2, address indicator 0x42 (route on SSN,
///              SSN present, no point code, no global title), SSN 8
/// 02 42 08     calling: the same
/// ll ..        data: length, TCAP message
/// ```
///
/// M3UA (RFC 4666): common header `version 1, reserved, class 1 (Transfer),
/// type 1 (DATA), length`, then one Protocol Data parameter (tag 0x0210):
/// `OPC, DPC, SI 3 (SCCP), NI 0, MP 0, SLS 0, user data`, padded to four
/// octets.
pub fn m3ua_frame(tcap: &[u8]) -> Vec<u8> {
    assert!(
        tcap.len() <= 255,
        "a UDT carries at most 255 octets of data"
    );
    let mut udt = vec![0x09, 0x00, 0x03, 0x05, 0x07];
    udt.extend_from_slice(&[0x02, 0x42, SUBSYSTEM]);
    udt.extend_from_slice(&[0x02, 0x42, SUBSYSTEM]);
    udt.push(tcap.len() as u8);
    udt.extend_from_slice(tcap);

    let mut parameter = Vec::new();
    parameter.extend_from_slice(&1u32.to_be_bytes()); // OPC
    parameter.extend_from_slice(&2u32.to_be_bytes()); // DPC
    parameter.extend_from_slice(&[3, 0, 0, 0]); // SI, NI, MP, SLS
    parameter.extend_from_slice(&udt);
    let parameter_length = 4 + parameter.len();
    let padding = (4 - parameter_length % 4) % 4;

    let mut frame = vec![1, 0, 1, 1];
    frame.extend_from_slice(&((8 + parameter_length + padding) as u32).to_be_bytes());
    frame.extend_from_slice(&0x0210u16.to_be_bytes());
    frame.extend_from_slice(&(parameter_length as u16).to_be_bytes());
    frame.extend_from_slice(&parameter);
    frame.resize(frame.len() + padding, 0);
    frame
}

fn attribute(line: &str, key: &str) -> Option<String> {
    let needle = format!(" {key}=\"");
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')? + start;
    Some(
        line[start..end]
            .replace("&quot;", "\"")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

fn parse_pdml(pdml: &str) -> Vec<Field> {
    pdml.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if !(line.starts_with("<field ") || line.starts_with("<proto ")) {
                return None;
            }
            Some(Field {
                name: attribute(line, "name")?,
                show: attribute(line, "show").unwrap_or_default(),
                showname: attribute(line, "showname").unwrap_or_default(),
                value: attribute(line, "value").unwrap_or_default(),
            })
        })
        .collect()
}

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Dissect one TCAP message with `tshark`, without judging the result.
/// `None` when the tools are missing.
pub fn dissect_unchecked(tcap: &[u8]) -> Option<Dissection> {
    if !tshark_available() {
        return None;
    }
    let frame = m3ua_frame(tcap);
    let directory: PathBuf = std::env::temp_dir().join(format!(
        "tcap-tshark-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).expect("temporary directory");
    let dump = directory.join("frame.txt");
    let capture = directory.join("frame.pcap");

    let mut text = String::from("000000");
    for byte in &frame {
        text.push_str(&format!(" {byte:02x}"));
    }
    text.push('\n');
    std::fs::write(&dump, text).expect("write hex dump");

    // -S: dummy SCTP DATA chunk, ports 2905 (M3UA), payload protocol id 3.
    let converted = Command::new("text2pcap")
        .args(["-q", "-S", "2905,2905,3"])
        .arg(&dump)
        .arg(&capture)
        .output()
        .expect("run text2pcap");
    assert!(
        converted.status.success(),
        "text2pcap failed: {}",
        String::from_utf8_lossy(&converted.stderr)
    );

    // An empty configuration directory keeps the run independent of whatever
    // protocol preferences the local Wireshark profile carries.
    let dissected = Command::new("tshark")
        .env("WIRESHARK_CONFIG_DIR", &directory)
        .env("XDG_CONFIG_HOME", &directory)
        .arg("-r")
        .arg(&capture)
        .args(["-T", "pdml"])
        .output()
        .expect("run tshark");
    assert!(
        dissected.status.success(),
        "tshark failed: {}",
        String::from_utf8_lossy(&dissected.stderr)
    );
    let _ = std::fs::remove_dir_all(&directory);

    let pdml = String::from_utf8_lossy(&dissected.stdout).into_owned();
    let fields = parse_pdml(&pdml);
    Some(Dissection { fields, pdml })
}

/// Dissect one TCAP message and check the frame is clean: Wireshark
/// recognised TCAP and reported nothing malformed, unknown or erroneous.
#[track_caller]
pub fn dissect(tcap: &[u8]) -> Option<Dissection> {
    let dissection = dissect_unchecked(tcap)?;
    dissection.clean();
    Some(dissection)
}

/// Bytes of a hand-written vector: hex octets, free whitespace, and `--`
/// comments running to the end of the line.
pub fn vector(text: &str) -> Vec<u8> {
    let octets: String = text
        .lines()
        .map(|line| line.split("--").next().unwrap_or(""))
        .flat_map(str::split_whitespace)
        .collect();
    hex::decode(&octets).unwrap_or_else(|e| panic!("bad hex in vector ({e}): {octets}"))
}

/// The known-answer check, both directions, against a vector that was
/// assembled by hand from the ASN.1 of Q.773 and not produced by this crate:
/// `message` must encode to exactly those bytes, and those bytes must decode
/// to exactly `message`. Returns the bytes.
#[track_caller]
pub fn known_answer(message: &TcapMessage, hand_assembled: &str) -> Vec<u8> {
    let expected = vector(hand_assembled);
    let encoded = tcap::encode(message).expect("encode");
    assert_eq!(
        hex::encode(&encoded),
        hex::encode(&expected),
        "encoding differs from the hand-assembled vector"
    );
    let decoded = tcap::decode(&expected).expect("decode the hand-assembled vector");
    assert_eq!(&decoded, message, "decoding the hand-assembled vector");
    expected
}

/// Decode bytes that must not be fully understood, and return the problem.
/// Also checks that the plain [`tcap::decode`] fails with the same problem.
#[track_caller]
pub fn problem(bytes: &[u8]) -> DecodeProblem {
    let detailed = match tcap::decode_detailed(bytes) {
        Decoded::Problem(problem) => *problem,
        Decoded::Complete(message) => panic!("decoded without a problem: {message:?}"),
    };
    match tcap::decode(bytes) {
        Err(tcap::TcapError::Malformed(plain)) => assert_eq!(*plain, detailed),
        other => panic!("tcap::decode did not report the problem: {other:?}"),
    }
    detailed
}
