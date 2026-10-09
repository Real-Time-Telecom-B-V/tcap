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
   opaque `Any` — decoded by MAP/CAP/INAP above. A `Reject` carries a typed
   `Problem` and an invoke ID that may be the not-derivable NULL.

3. **Dialogue portion** (`DialoguePortion`) — the `EXTERNAL` carrying a dialogue
   PDU (AARQ / AARE / ABRT, or AUDT in a Unidirectional) used to negotiate an
   application context, or user information in a user-defined syntax. The
   `EXTERNAL` is held verbatim; `parse()` reads it strictly.

## Encoding

TCAP is BER (X.690). The crate leans on [`rasn`](https://docs.rs/rasn) for the
`AsnType` / `Encode` / `Decode` derives; the tags in the derive attributes are
the Q.773 assignments, so `rasn::ber::encode` produces wire-correct bytes once
`encode` has checked that the message is one Q.773 allows.

```rust
pub fn encode(msg: &TcapMessage) -> Result<Vec<u8>, TcapError>;
pub fn decode(bytes: &[u8]) -> Result<TcapMessage, TcapError>;
pub fn decode_detailed(bytes: &[u8]) -> Decoded;
```

`TcapError` is a `thiserror` enum: `Malformed` (a received message that was not
fully understood, with the `DecodeProblem`), `InvalidMessage` (a message
`encode` refuses), and the encode / decode failures of `rasn`.

## Decoding

Decoding does not go through `rasn::ber::decode` on the whole message. `rasn`
0.28 returns a SEQUENCE OF without the elements it could not read, and ignores
octets after the outermost value; for TCAP that is a component the TC-user never
sees and a peer left waiting on its invoke timer. And a pass / fail answer is
not enough either: Q.774 has the receiver answer a damaged message, and the
answer needs the transaction IDs and the invoke ID out of the damage.

So the decoder works in the three stages the procedures distinguish:

1. **Transaction portion** — read with a small BER element reader: message
   type, transaction IDs, the framing of the dialogue and component portions,
   and nothing after the end. A problem here has a P-Abort cause (Q.772 2.3).
2. **Dialogue portion** — when present it has to parse. A problem here is an
   "abnormal dialogue" (Q.774 3.2.2.1); the components go with it.
3. **Components** — one at a time: `rasn` decodes the component, the value is
   encoded again, and the two encodings are walked side by side so that every
   element on the wire is accounted for. A problem here has a general problem
   code (Q.772 3.7.1); the components before it stand, those after it are
   discarded (Q.774 3.2.2.2).

The result is `Decoded::Complete(message)` or `Decoded::Problem(problem)`. A
`DecodeProblem` says which sub-layer detected the problem, gives the P-Abort
cause or the general problem, the IDs that could be recovered, and builds the
`Abort` or the `Reject` Q.774 requires.

## Why the parameters stay opaque

TCAP's job is to *frame and route* — to say "this is invoke id 3 of operation
45, and here are its argument octets" — not to understand the argument. Keeping
the `Invoke.parameter`, `ReturnResultValue.parameter` and `ReturnError.parameter`
as opaque `Any` bytes means this crate is a small, stable transaction/component
codec. (What TCAP itself has to act on is typed: the P-Abort cause, the Reject
problem, the dialogue PDUs.) The MAP/CAP/INAP layer that knows what operation 45
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
