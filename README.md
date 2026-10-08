# tcap

[![crates.io](https://img.shields.io/crates/v/tcap.svg)](https://crates.io/crates/tcap)
[![docs.rs](https://docs.rs/tcap/badge.svg)](https://docs.rs/tcap)
[![CI](https://github.com/Real-Time-Telecom-B-V/tcap/actions/workflows/ci.yml/badge.svg)](https://github.com/Real-Time-Telecom-B-V/tcap/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A **TCAP (Transaction Capabilities Application Part)** codec — the SS7
transaction-and-component layer that carries MAP, CAP, and INAP dialogues.
Implements the ITU-T **Q.771–Q.775** message set over BER (X.690): the five
transaction PDUs, the component sub-layer, and the dialogue portion. Shipped as
both a **Rust crate** (`cargo add tcap`) and a **Rust-backed Python wheel**
(`pip install ss7-tcap`, imported as `tcap`) from one source tree, one version.

TCAP is where an SS7 application conversation lives. Below it, **SCCP** provides
the global-title routing and **M3UA**/**MTP3** the network transport; above it,
**MAP** / **CAP** / **INAP** ride as the operations inside its components. This
crate is the **wire format** only — no state machine, no dialogue-handling
runtime, no I/O — so it composes into whatever transaction coordinator sits on
top.

```rust
use tcap::{Begin, Component, Invoke, OperationCode, TcapMessage};

// Build a TCAP Begin carrying a single Invoke (e.g. a MAP operation) …
let invoke = Invoke {
    invoke_id: 1,
    linked_id: None,
    operation_code: OperationCode::Local(45), // application-defined operation
    parameter: None,                          // MAP/CAP/INAP argument goes here
};

let begin = Begin {
    otid: vec![0x00, 0x00, 0x00, 0x01].into(), // originating transaction id
    dialogue_portion: None,
    components: Some(vec![Component::Invoke(invoke)]),
};

// … encode it to Q.773-compliant BER, and decode it back.
let wire = tcap::encode(&TcapMessage::Begin(begin)).unwrap();
assert_eq!(wire[0], 0x62); // [APPLICATION 2] CONSTRUCTED = Begin
let msg = tcap::decode(&wire).unwrap();
assert!(matches!(msg, TcapMessage::Begin(_)));
```

## A decode never loses anything silently

`decode` returns a message only when every part of it was understood and every
octet of the input belongs to it. A component that cannot be read, a malformed
dialogue portion, an element out of place, octets after the end of the message:
each makes the call fail with `TcapError::Malformed`. It does not come back as
a message that simply lacks the part.

A TCAP receiver has to do more than refuse, though. Q.774 has it answer: an
Abort addressed with the originating transaction ID of the damaged message, or a
Reject with the invoke ID of the component that could not be read. So the error
carries a `DecodeProblem`, and `decode_detailed` returns the same thing as a
plain value:

```rust
use tcap::{Decoded, Fault, GeneralProblem, Reject};

// A Begin with two Invokes; the second stops after its invoke ID.
let wire = [
    0x62, 0x15, 0x48, 0x04, 0x00, 0x00, 0x10, 0x01, 0x6c, 0x0d,
    0xa1, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x3a, // invoke 1, operation 58
    0xa1, 0x03, 0x02, 0x01, 0x02,                   // invoke 2, no operation code
];
let Decoded::Problem(problem) = tcap::decode_detailed(&wire) else { unreachable!() };

assert!(matches!(problem.fault, Fault::Component { index: 1, .. }));
// The Reject Q.774 asks for, with the invoke ID that could still be read.
assert_eq!(
    problem.reject(),
    Some(Reject::general(Some(2), GeneralProblem::MISTYPED_COMPONENT))
);
// The component before the faulty one stands; those after it are discarded.
assert_eq!(problem.partial.unwrap().components().len(), 1);
```

| Where the problem is | Detected by | `DecodeProblem` gives | Response (Q.774) |
|---|---|---|---|
| Transaction portion: message type, transaction IDs, framing, trailing octets | transaction sub-layer (3.3.4) | `p_abort_cause()`, `otid`, `dtid`, `message_type` | `abort()`: an Abort with the P-Abort cause, when the originating ID is derivable and the message is a Begin, a Continue or of unknown type. Otherwise the message is discarded. |
| Dialogue portion present and malformed | component sub-layer (3.2.2.1) | `Fault::DialoguePortion` | `abort()`: an Abort carrying an ABRT APDU, abort-source dialogue-service-provider. The components are discarded. |
| A component | component sub-layer (3.2.2.2) | component index and type, `invoke_id` if readable, the general problem, `partial` | `reject()`: a Reject component, for the TC-user's next Continue or End. None when the faulty component is itself a Reject. |

Which transaction the IDs belong to, and whether it exists, is for the state
machine above; this crate has none.

## What it covers

| Layer | Types |
|---|---|
| **Transaction** (Q.773 §3.1) | `TcapMessage` — `Unidirectional` · `Begin` · `End` · `Continue` · `Abort`, each with its APPLICATION-class tag and OTID/DTID transaction identifiers (1 to 4 octets). `Abort` carries an `AbortReason`: a `PAbortCause` or the user abort information. |
| **Component** (Q.773 §3.1) | `Component` — `Invoke` · `ReturnResultLast` · `ReturnResultNotLast` · `ReturnError` · `Reject`, with `OperationCode` / `ErrorCode` (local or global OID) and opaque parameters. `Reject` carries a typed `Problem` (`GeneralProblem` / `InvokeProblem` / `ReturnResultProblem` / `ReturnErrorProblem`) and an invoke ID that may be the not-derivable NULL. |
| **Dialogue** (Q.773 §3.2) | `DialoguePortion` — the `EXTERNAL` carrying a dialogue PDU (`DialoguePdu`: `Aarq` / `Aare` / `Abrt` / `Audt`) or user information. Builders (`aarq`, `aare_accept`, `aare_reject`, `abrt`, `audt`, `from_pdu`, `from_external`) and a strict reader, `parse()`. |
| **Decode result** | `Decoded` · `DecodeProblem` · `Fault` · `Sublayer`, see above. |

The API is three free functions plus the types:

```rust
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError>;
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError>;
pub fn decode_detailed(bytes: &[u8]) -> Decoded;
```

`encode` refuses a message Q.773 does not allow (a transaction ID that is not 1
to 4 octets, a component portion without components, a result sequence without
its parameter, a malformed dialogue portion) instead of putting it on the wire.

Component parameters (the MAP/CAP/INAP argument) are kept as opaque `Any`
bytes — TCAP delimits and routes them; the application layer above decodes
them. That keeps this crate a focused transaction/component codec rather than a
full MAP stack.

### Dialogue portion

The dialogue portion carries the ACSE-style AARQ / AARE / ABRT PDUs that negotiate
the **application context** (the MAP/CAP operation set), or the AUDT of a
Unidirectional. Build one with a typed builder and hand it straight to a
transaction; parse a received one back to read the context and result:

```rust
use rasn::types::Oid;
use tcap::{Begin, DialoguePortion, DialoguePdu, TcapMessage};

// MAP shortMsgGateway v3 — the SRI-SM application context.
let ac = Oid::new(&[0, 4, 0, 0, 1, 0, 20, 3]).unwrap();

let begin = Begin {
    otid: vec![0, 0, 0, 1].into(),
    dialogue_portion: Some(DialoguePortion::aarq(ac)), // typed AARQ
    components: None,
};
let wire = tcap::encode(&TcapMessage::Begin(begin)).unwrap();

// On the receiving side, read the negotiated context back out.
if let TcapMessage::Begin(b) = tcap::decode(&wire).unwrap() {
    let dp = b.dialogue_portion.unwrap();
    match dp.dialogue_pdu() {
        Ok(Some(DialoguePdu::Aarq { application_context_name, .. })) => {
            assert_eq!(application_context_name.as_ref(), &[0, 4, 0, 0, 1, 0, 20, 3]);
        }
        _ => unreachable!(),
    }
}
```

Reading a dialogue portion has three outcomes that are never confused:

- `Ok(DialogueContent::Pdu(..))` (`dialogue_pdu()`: `Ok(Some(..))`): a
  well-formed AARQ, AARE, ABRT or AUDT.
- `Ok(DialogueContent::Unmodelled(external))` (`Ok(None)`): a well-formed
  `EXTERNAL` carrying something else, such as user information in a
  user-defined abstract syntax in an Abort. The `External` is handed over.
- `Err(DialogueError)`: the portion is there and is malformed. `decode` does not
  return such a message; it reports `Fault::DialoguePortion`.

An absent portion is the `None` of the message's `dialogue_portion`.
`user_information` is a typed `Vec<External>`, and `DialoguePortion { external }`
still holds the BER of the `EXTERNAL` verbatim.

## Conformance

The encodings are checked against ITU-T Q.773 (06/97) in three independent ways,
for every message type and every component type:

- a byte vector assembled by hand from the ASN.1, with its derivation in a
  comment (`tests/wire_vectors.rs`, `tests/dialogue_vectors.rs`);
- Wireshark's dissection of the bytes this crate emits, asserted field by field,
  with no malformed or BER error marker (`tests/common/mod.rs`; needs `tshark`
  and `text2pcap`, skipped with a `SKIP` line when they are missing, or failing
  when `TCAP_REQUIRE_TSHARK=1`);
- a decode of bytes this crate did not produce: encodings made by pyasn1 from a
  transcription of the ASN.1 (`scripts/foreign_vectors.py`,
  `tests/foreign_vectors.rs`), in definite and indefinite length form.

What the decoder reports for damaged messages follows Q.772 Tables 1 and 2 and
Q.774 Tables 5 and 7 case by case (`tests/decode_problems.rs`).

Two deliberate tolerances on receive, where nothing is lost by reading: length
fields and INTEGERs in more octets than needed, and a ReturnResult whose result
holds the operation code without a parameter. The encoder emits neither.

## Where it fits

```
   map / cap / inap          (operations inside the components)
        │
   tcap                       (this crate — transactions + components, BER)
        │
   sccp                       (global-title routing)
        │
   m3ua / mtp3                (SS7 network transport)
```

More: [`docs/OVERVIEW.md`](docs/OVERVIEW.md).

## Python

The same codec is available as a Rust-backed wheel. The name `tcap` is taken on
PyPI, so the distribution is **`ss7-tcap`** — but the import name is `tcap`:

```bash
pip install ss7-tcap
```

```python
import tcap

# Build a Begin carrying an Invoke (e.g. a MAP operation) …
begin = tcap.Begin(
    b"\x00\x00\x00\x01",  # originating transaction id (OTID)
    components=[
        tcap.Invoke(1, tcap.OperationCode.local(45), parameter=b"\x04\x03\x01\x02\x03"),
    ],
)

wire = begin.encode()          # Q.773-compliant BER bytes
assert wire[0] == tcap.TAG_BEGIN  # 0x62 = [APPLICATION 2] CONSTRUCTED

msg = tcap.decode(wire)        # -> a Begin
assert msg.components[0].operation_code == tcap.OperationCode.local(45)
```

The Python surface mirrors the Rust one: the transaction messages (`Begin`,
`Continue`, `End`, `Abort`, `Unidirectional`), the components (`Invoke`,
`ReturnResult`, `ReturnError`, `Reject`), `OperationCode` / `ErrorCode`
(local integer or global OID), plus `encode()` / `decode()` and the Q.773
tag, component-type, P-Abort cause and problem constants. Operation arguments
are `bytes`, exactly as the Rust codec keeps them. `Abort` takes
`p_abort_cause=` (an int) or `dialogue_portion=`; `Reject` takes an invoke ID
(or `None`), a problem class and a problem code.

`decode()` raises `TcapError` when any part of a message was not understood, and
the exception's `.problem` is the `DecodeProblem` (`.sublayer`, `.fault`,
`.p_abort_cause`, `.general_problem`, `.invoke_id`, `.otid`, `.dtid`,
`.partial`, and `.abort()` / `.reject()` for the response). `decode_detailed()`
returns the message or the `DecodeProblem` without raising.

The dialogue portion has typed helpers — `dialogue_aarq(oid)`,
`dialogue_aare_accept(oid)`, `dialogue_aare_reject(oid, source, value)`,
`dialogue_abrt(source)`, `dialogue_audt(oid)` return the `EXTERNAL` bytes for a
message's `dialogue_portion=`, and `parse_dialogue_portion(bytes)` reads one
back into a `DialoguePdu` (`.pdu_type`, `.application_context`, `.version1`,
`.result`, `.abort_source`), returns `None` for a well-formed portion that is
not a dialogue PDU, and raises for a malformed one:

```python
dp = tcap.dialogue_aarq([0, 4, 0, 0, 1, 0, 20, 3])   # MAP SRI-SM context
begin = tcap.Begin(b"\x00\x00\x00\x01", dialogue_portion=dp)

pdu = tcap.parse_dialogue_portion(tcap.decode(begin.encode()).dialogue_portion)
assert pdu.pdu_type == "AARQ" and pdu.application_context == [0, 4, 0, 0, 1, 0, 20, 3]
```

The module is declared `gil_used = false`, so it loads on free-threaded CPython.

## Development

Rust:

```bash
cargo test                                  # unit + integration + doctest
cargo test --features python                # the PyO3 bindings
cargo clippy --all-targets -- -D warnings
cargo clippy --features python --lib -- -D warnings
cargo bench --no-run                        # keep the benches compiling
cargo run --release --example leak_check    # counting-allocator leak gate → PASS
cargo deny check
```

Python (wheel):

```bash
python -m venv .venv && . .venv/bin/activate
pip install maturin pytest
maturin develop                             # build + install the extension
pytest python/tests -q
```

## License

MIT — see [LICENSE](LICENSE).
