# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). See
[VERSIONING.md](VERSIONING.md) for the policy.

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

[1.0.0]: https://github.com/Real-Time-Telecom-B-V/tcap/releases/tag/v1.0.0
