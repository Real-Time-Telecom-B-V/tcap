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
  for application-context negotiation, carried opaquely.
- **`TcapError`** — `thiserror` enum over encode / decode / invalid-message /
  missing-field, with `From` conversions off `rasn`'s error types.
- `Display` for `TcapMessage`, `Component`, `OperationCode`, and `ErrorCode`.
- Tests covering round-trip encode/decode of every transaction and component
  type, the Q.773 tag assignments, and error/display paths.

[1.0.0]: https://github.com/Real-Time-Telecom-B-V/tcap/releases/tag/v1.0.0
