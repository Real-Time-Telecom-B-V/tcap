# Versioning

`tcap` follows [Semantic Versioning 2.0.0](https://semver.org/). The public API
— the `encode` / `decode` / `decode_detailed` functions, the `TcapMessage`
transaction types, the `Component` / `Invoke` / `ReturnResult` / `ReturnError` /
`Reject` component types, `OperationCode` / `ErrorCode`, the P-Abort cause and
problem codes, `DialoguePortion` and the dialogue PDU types, `Decoded` /
`DecodeProblem`, and `TcapError` — is the contract, along with the wire format
they produce.

## The git tag is the source of truth

`Cargo.toml`'s `version` matches the release tag; the release workflow's
`verify-version` job refuses to publish if they disagree. Bump `version`, commit,
tag `vX.Y.Z`, push the tag.

## Post-1.0 rule

- **MAJOR** — remove/rename/re-signature a `pub` item, or change the bytes
  produced for an existing input (a wire-format change).
- **MINOR** — backward-compatible additions (new component/transaction variants,
  new helper methods, new operation/error forms).
- **PATCH** — bug fixes, docs, behaviour-neutral dependency bumps.
