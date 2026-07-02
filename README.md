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

## What it covers

| Layer | Types |
|---|---|
| **Transaction** (Q.773) | `TcapMessage` — `Unidirectional` · `Begin` · `End` · `Continue` · `Abort`, each with its APPLICATION-class tag and OTID/DTID transaction identifiers. |
| **Component** (Q.773 §3.2) | `Component` — `Invoke` · `ReturnResultLast` · `ReturnResultNotLast` · `ReturnError` · `Reject`, with `OperationCode` / `ErrorCode` (local or global OID) and opaque parameters. |
| **Dialogue** (Q.773) | `DialoguePortion` — carries the `EXTERNAL` dialogue PDU (AARQ/AARE/ABRT) for application-context negotiation. |

The API is two free functions plus the types:

```rust
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError>;
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError>;
```

Component parameters (the MAP/CAP/INAP argument, the dialogue `EXTERNAL`, a
`Reject` problem) are kept as opaque `Any` bytes — TCAP delimits and routes them;
the application layer above decodes them. That keeps this crate a focused
transaction/component codec rather than a full MAP stack.

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
tag / component-type constants. Opaque fields (operation arguments, the dialogue
`EXTERNAL`, a `Reject` problem) are `bytes`, exactly as the Rust codec keeps them.
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
