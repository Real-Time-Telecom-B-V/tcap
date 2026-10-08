# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). See
[VERSIONING.md](VERSIONING.md) for the policy.

## [2.0.0]

Decoding no longer loses anything silently, and the parts of a message TCAP
itself has to act on are typed. 1.0.0 handed the whole message to `rasn`, which
returns a SEQUENCE OF without the elements it could not read and ignores octets
after the outermost value; and its tests compared the encoder with the decoder,
which a mistake the two share passes. Every message type and component type is
now checked against a byte vector assembled by hand from the ASN.1 of ITU-T
Q.773 (06/97), against the fields Wireshark reads back from the bytes this crate
emits, and against encodings produced by a different encoder (pyasn1).

**Several things in 1.0.0 were wrong on the wire**, listed one by one below.
Anything that relied on a truncated decode, or that sent an Abort with a reason,
has to be looked at.

### Fixed
- **A component that could not be decoded disappeared.** A component portion
  whose only component was malformed decoded as an empty list
  (`62 0D 48 04 00 00 10 01 6C 05 A1 03 02 01 02` gave `components:
  Some([])`), and a malformed last component after a good one was dropped,
  whether an Invoke or a ReturnResult with a malformed result. The TC-user never
  saw the component and no Reject went back. `decode` now fails; the error says
  which component, with the general problem and the invoke ID for the Reject
  that Q.774 3.2.2.2 requires, and hands over the components before it.
- **An element in the wrong form was read as a different value.** `rasn` does
  not check the primitive / constructed bit on an implicit tag: a Reject whose
  problem arrived as `A0 03 02 01 01` (a constructed `[0]` around an INTEGER)
  decoded as general problem 131329, the three content octets taken for the
  number. Every element of a component now has to have the form Q.773 gives it.
- **A Reject with the not-derivable invoke ID could not be decoded** (it was
  one of the components that disappeared), and could not be built: `Reject` had
  an integer invoke ID only. Q.773 has `invokeID CHOICE { derivable
  InvokeIdType, not-derivable NULL }`; `Reject::invoke_id` is now an `Option`
  and `None` is the NULL (`A4 05 05 00 80 01 02`).
- **Octets after the end of the message were ignored.** They are now a badly
  formatted transaction portion ("Length indicator value does not correspond to
  length of message", Q.772 Table 1): SCCP delivers the user data with its
  length, so there is no padding to tolerate.
- **A malformed dialogue PDU read as "no dialogue PDU"**, the same answer as for
  a well-formed portion in a syntax the crate does not model.
  `DialoguePortion::parse` now tells the three apart, and `decode` reports a
  malformed portion as `Fault::DialoguePortion` (Q.774 3.2.2.1). The 1.0.0
  parser also skipped whatever followed the members it knew and did not read
  the indefinite length form.
- **An Abort could not carry a correct reason.** `Abort::reason` was an opaque
  value under an implicit `[APPLICATION 10]`: given the encoding of a P-Abort
  cause it produced `6A 03 4A 01 03`, a constructed wrapper around the cause,
  and given a dialogue portion it produced `6A ..` around the `EXTERNAL`
  where `6B ..` belongs. Not interoperable before: Wireshark reads either as a
  P-Abort cause of nonsense length. And a received Abort with user abort
  information (`6B ..`) failed to decode. The reason is now `AbortReason`,
  `PAbort(PAbortCause)` (`4A 01 nn`) or `UAbort(DialoguePortion)` (`6B ..`).
- **Transaction IDs of any length were emitted and accepted**, including none
  at all. Q.773 has `SIZE (1..4)`; `encode` refuses another length and the
  decoder reports an incorrect transaction portion.
- **A component portion with no components was emitted (`6C 00`) and accepted.**
  Q.773 has `SIZE (1..MAX)`; `encode` refuses it (use `components: None`) and
  the decoder reports an incorrect transaction portion ("Component Portion Tag
  present, but no components", Q.772 Table 1).
- **Invoke IDs outside -128..127 were emitted** in two octets. `InvokeId` is now
  `i8` (`InvokeIdType ::= INTEGER (-128..127)`).
- **A result sequence without its parameter was emitted** (`30 03 02 01 2D`).
  In Q.773 the parameter of `result SEQUENCE { operationCode, parameter }` is
  not OPTIONAL. `encode` refuses it: leave `result` as `None` for an operation
  that returns nothing. The decoder still reads it, since receivers in the
  field do and nothing is lost.
- **`AssociateResult::RejectedTransient` is gone.** Q.773 defines `accepted (0)`
  and `reject-permanent (1)` only; value 2 is ACSE's and was never a TC value.
- **Object identifiers under arc 2 with a second arc of 48 or more** were
  encoded with a truncated first octet in the dialogue portion (2.999.1 is
  `88 37 01`).
- What the decoder already refused (an unknown message type, an unknown
  component tag, elements out of order, a malformed component before a good
  one) came back as a text; it now comes back with the P-Abort cause or the
  general problem Q.772 gives it and the IDs for the response.

### Changed
- `decode` fails with the new `TcapError::Malformed(Box<DecodeProblem>)` where
  it used to return a truncated message or `TcapError::DecodeError`.
- `Abort { dtid, reason: Option<AbortReason> }`; build with `Abort::p_abort` and
  `Abort::u_abort`.
- `Reject { invoke_id: Option<InvokeId>, problem: Problem }`; build with
  `Reject::general`, `::invoke`, `::return_result`, `::return_error`.
- `InvokeId` is `i8` (was `i64`).
- `DialoguePortion::dialogue_pdu` returns `Result<Option<DialoguePdu>,
  DialogueError>` (was `Option<DialoguePdu>`).
- `DialoguePdu::*::user_information` is `Option<Vec<External>>` (was the raw
  content octets); `ProtocolVersion` has `Other(..)` for a version list without
  version 1 and is no longer `Copy`.
- `encode` validates the message and returns `TcapError::InvalidMessage` for
  what Q.773 does not allow.
- Python: `Abort(dtid, *, p_abort_cause=None, dialogue_portion=None)` (was
  `reason=bytes`); `Reject(invoke_id, problem_type, problem_code)` (was
  `problem=bytes`); `parse_dialogue_portion` raises `TcapError` for a malformed
  portion; invoke IDs outside -128..127 raise `OverflowError`.

### Added
- **`decode_detailed`** → `Decoded::Complete(message)` or
  `Decoded::Problem(problem)`. A **`DecodeProblem`** says which sub-layer
  detected the problem (`sublayer()`), carries the `Fault` (transaction portion
  with its `PAbortCause`, dialogue portion, or a component with its index, type,
  invoke ID and `GeneralProblem`), the message type, the transaction IDs that
  are derivable, and for a component fault the message up to that component.
  **`DecodeProblem::abort()`** and **`DecodeProblem::reject()`** build the
  response Q.774 requires, or return `None` where it requires none (a damaged
  End or Abort, an originating transaction ID that is not derivable, a
  malformed Reject).
- `PAbortCause`, `GeneralProblem`, `InvokeProblem`, `ReturnResultProblem`,
  `ReturnErrorProblem` with the named values of Q.773 Tables 12 and 26 to 29;
  `Problem`; `MessageType`; `ComponentType`; `TransactionId` at the crate root.
- `TcapMessage::message_type()`, `otid()`, `dtid()`, `dialogue_portion()`,
  `components()`; `Component::component_type()`, `invoke_id()`.
- Dialogue: `DialoguePdu::Audt` and `DialoguePortion::audt` (the unstructured
  dialogue of a Unidirectional); `DialoguePortion::parse` → `DialogueContent`;
  `External` / `ExternalEncoding` with `encode` / `decode`; `from_external`;
  `aare_reject`; `abnormal_dialogue()` (Q.774 3.2.2.1) and
  `no_common_dialogue_portion(ac)` (Q.774 3.2.3); the diagnostic constants on
  `AssociateSourceDiagnostic`; `DialogueError`; `UNIDIALOGUE_AS_OID`.
- The decoder reads the indefinite length form everywhere, a dialogue PDU
  carried octet-aligned or as a bit string, and an AARQ without the DEFAULT
  protocol version.
- Python: `decode_detailed`, `DecodeProblem`, `TcapError.problem`,
  `dialogue_aare_reject`, `dialogue_audt`, `DialoguePdu.version1`, the
  `P_ABORT_*`, `PROBLEM_*` and `GENERAL_PROBLEM_*` constants.
- Tests: hand-assembled vectors with their derivation, a Wireshark dissection
  harness (TCAP in SCCP UDT in M3UA, skipped with a message when `tshark` is
  missing), pyasn1-encoded vectors with the script that makes them, and the
  Q.772 / Q.774 tables case by case.

## [1.0.0]

First release — a TCAP codec for the SS7 transaction and component layer, per
ITU-T Q.771–Q.775.

### Added
- **`encode`** / **`decode`** — BER (X.690) codec for a whole TCAP message.
- **`TcapMessage`** — the transaction sub-layer: `Unidirectional`, `Begin`,
  `End`, `Continue`, `Abort`, each with its Q.773 APPLICATION-class tag and
  OTID/DTID transaction identifiers.
- **`Component`** — the component sub-layer: `Invoke`, `ReturnResultLast`,
  `ReturnResultNotLast`, `ReturnError`, `Reject`, with `ReturnResult` /
  `ReturnResultValue` values.
- **`OperationCode`** / **`ErrorCode`** — local (integer) or global (OID) forms.
- **`DialoguePortion`** — the `EXTERNAL`-wrapped dialogue PDU (AARQ/AARE/ABRT)
  for application-context negotiation. A **typed** layer sits over the raw
  `external` escape hatch: the `DialoguePdu` enum (`Aarq` / `Aare` / `Abrt`) with
  `AssociateResult`, `AssociateSourceDiagnostic`, `AbortSource`, and
  `ProtocolVersion`; byte-exact builders `aarq(ac)` / `aare_accept(ac)` /
  `abrt(source)` / `from_pdu(pdu)`; and a `dialogue_pdu()` parser that reads a
  received portion back into the typed form (round-trips). `user_information`
  stays opaque; structured-dialogue AUDT is out of scope.
- **`TcapError`** — `thiserror` enum over encode / decode / invalid-message /
  missing-field, with `From` conversions off `rasn`'s error types.
- `Display` for `TcapMessage`, `Component`, `OperationCode`, and `ErrorCode`.
- Tests covering round-trip encode/decode of every transaction and component
  type, the Q.773 tag assignments, and error/display paths.
- **Python bindings** (`pip install ss7-tcap`, imported as `tcap`; feature
  `python`) — the transaction messages (`Begin`, `Continue`, `End`, `Abort`,
  `Unidirectional`), the components (`Invoke`, `ReturnResult`, `ReturnError`,
  `Reject`), `OperationCode` / `ErrorCode`, `encode()` / `decode()`, and the
  Q.773 tag / component-type constants. Opaque fields are `bytes`. The dialogue
  portion has typed helpers — `dialogue_aarq` / `dialogue_aare_accept` /
  `dialogue_abrt` builders and a `parse_dialogue_portion()` reader returning a
  `DialoguePdu` (with `pdu_type` / `application_context` / `result` /
  `abort_source`), plus the `ABORT_SOURCE_*` constants. Declared
  `gil_used = false` for free-threaded CPython. A `register(py, parent)` entry
  point mounts `tcap` as a submodule of a host extension. (The crates.io crate
  stays `tcap`; only the PyPI distribution is `ss7-tcap`, as `tcap` is taken.)
- **Quality bar** — criterion benches (`benches/codec.rs`: Begin-with-Invoke and
  End-with-ReturnResult encode/decode), a counting-allocator leak check
  (`examples/leak_check.rs` + `scripts/mem_leak_test.sh`), pytest parity tests,
  and CI (fmt / clippy both faces / test both faces / bench-compile / leak gate /
  wheel + pytest on 3.9 & 3.13 + free-threaded).

[2.0.0]: https://github.com/Real-Time-Telecom-B-V/tcap/releases/tag/v2.0.0
[1.0.0]: https://github.com/Real-Time-Telecom-B-V/tcap/releases/tag/v1.0.0
