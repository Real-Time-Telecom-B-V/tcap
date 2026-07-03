# tcap — overview

A pure-Rust codec for the **Transaction Capabilities Application Part** (TCAP),
the SS7 layer that turns a sequence of messages into an application *dialogue*.
It implements the ITU-T **Q.771–Q.775** message set over BER (X.690) and nothing
else: no state machine, no dialogue coordinator, no I/O — so it drops into
whatever runtime owns the transactions.

## The idea

An SS7 application conversation — a MAP `sendRoutingInfoForSM`, a CAP call
control dialogue, an INAP trigger — is a series of TCAP messages sharing a
transaction identifier. TCAP has three nested concerns, and this crate models
each as plain data:

1. **Transaction sub-layer** (`TcapMessage`) — the PDU that opens, continues,
   and closes a dialogue, tagged APPLICATION-class per Q.773:

   | PDU | Tag | Role |
   |---|---|---|
   | `Unidirectional` | `[APPLICATION 1]` | fire-and-forget, no transaction |
   | `Begin` | `[APPLICATION 2]` | open a transaction (carries an OTID) |
   | `End` | `[APPLICATION 4]` | close it (carries the DTID) |
   | `Continue` | `[APPLICATION 5]` | keep it open (OTID + DTID) |
   | `Abort` | `[APPLICATION 7]` | tear it down (DTID + reason) |

2. **Component sub-layer** (`Component`) — the operations inside a PDU:
   `Invoke`, `ReturnResultLast` / `ReturnResultNotLast`, `ReturnError`,
   `Reject`. Each `Invoke`/result names an `OperationCode` (local integer or a
   global OID); each error names an `ErrorCode`. The actual argument bytes are
   opaque `Any` — decoded by MAP/CAP/INAP above.

3. **Dialogue portion** (`DialoguePortion`) — the `EXTERNAL`-wrapped dialogue
   PDU (AARQ / AARE / ABRT) used to negotiate an application context. Carried
   opaquely so the codec stays independent of any particular context set.

## Encoding

TCAP is BER (X.690). The crate leans on [`rasn`](https://docs.rs/rasn) for the
`AsnType` / `Encode` / `Decode` derives; the tags in the derive attributes are
the Q.773 assignments, so `rasn::ber::encode` directly produces wire-correct
bytes. Two functions wrap it:

```rust
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError>;
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError>;
```

`TcapError` is a `thiserror` enum distinguishing encode, decode, and structural
(invalid-message / missing-field) failures, with `From` conversions off
`rasn`'s own error types.

## Why the parameters stay opaque

TCAP's job is to *frame and route* — to say "this is invoke id 3 of operation
45, and here are its argument octets" — not to understand the argument. Keeping
the `Invoke.parameter`, `ReturnResultValue.parameter`, `Reject.problem`, and the
dialogue `EXTERNAL` as opaque `Any` bytes means this crate is a small, stable
transaction/component codec. The MAP/CAP/INAP layer that knows what operation 45
*means* decodes those bytes itself. This is the same split ITU-T draws between
Q.773 (the transaction/component portions) and the application-part specs above
it.

## Where it fits

```
   map / cap / inap    (operations carried inside components)
        │
   tcap                (this crate — transaction + component portions, BER)
        │
   sccp                (global-title routing)
        │
   m3ua / mtp3         (SS7 network transport)
```

Stack position: SCCP carries TCAP; MAP builds on it.
