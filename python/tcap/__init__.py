"""tcap — Rust-backed TCAP (ITU-T Q.771–Q.775) BER codec for Python.

TCAP (Transaction Capabilities Application Part) is the SS7 transaction-and-
component layer that carries MAP, CAP, and INAP dialogues. This package exposes
the same BER codec the Rust crate (``cargo add tcap``) ships, from one source
tree / one version: build a transaction (``Begin`` / ``Continue`` / ``End`` /
``Abort`` / ``Unidirectional``) with its components (``Invoke`` /
``ReturnResult`` / ``ReturnError`` / ``Reject``), call ``.encode()`` for wire
bytes, and ``tcap.decode(bytes)`` to parse. ``decode`` raises ``TcapError``
when any part of a message was not understood, with the ``DecodeProblem`` (the
P-Abort cause or Reject problem and the ids Q.774 needs for the answer) as its
``problem`` attribute; ``decode_detailed`` returns it without raising.

The wire work (BER encode/decode, tag handling) runs in Rust; Python just builds
and inspects messages. Operation arguments and the dialogue ``EXTERNAL`` are
carried opaquely as ``bytes`` — the application layer above (e.g. a MAP stack)
decodes them.

Note: the PyPI distribution is ``ss7-tcap`` (``tcap`` is taken), but the import
name is ``tcap``.
"""

from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version

from ._tcap import (
    ABORT_SOURCE_PROVIDER,
    ABORT_SOURCE_USER,
    COMPONENT_INVOKE,
    COMPONENT_REJECT,
    COMPONENT_RETURN_ERROR,
    COMPONENT_RETURN_RESULT_LAST,
    COMPONENT_RETURN_RESULT_NOT_LAST,
    GENERAL_PROBLEM_BADLY_STRUCTURED_COMPONENT,
    GENERAL_PROBLEM_MISTYPED_COMPONENT,
    GENERAL_PROBLEM_UNRECOGNIZED_COMPONENT,
    P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION,
    P_ABORT_INCORRECT_TRANSACTION_PORTION,
    P_ABORT_RESOURCE_LIMITATION,
    P_ABORT_UNRECOGNIZED_MESSAGE_TYPE,
    P_ABORT_UNRECOGNIZED_TRANSACTION_ID,
    PROBLEM_GENERAL,
    PROBLEM_INVOKE,
    PROBLEM_RETURN_ERROR,
    PROBLEM_RETURN_RESULT,
    TAG_ABORT,
    TAG_BEGIN,
    TAG_CONTINUE,
    TAG_END,
    TAG_UNIDIRECTIONAL,
    Abort,
    Begin,
    Continue,
    DecodeProblem,
    DialoguePdu,
    End,
    ErrorCode,
    Invoke,
    OperationCode,
    Reject,
    ReturnError,
    ReturnResult,
    TcapError,
    Unidirectional,
    decode,
    decode_detailed,
    dialogue_aarq,
    dialogue_aare_accept,
    dialogue_aare_reject,
    dialogue_abrt,
    dialogue_audt,
    encode,
    parse_dialogue_portion,
)

try:
    # The PyPI distribution is named `ss7-tcap`, not `tcap`.
    __version__ = version("ss7-tcap")
except PackageNotFoundError:  # running from a source checkout without an installed dist
    __version__ = "0.0.0+unknown"

__all__ = [
    # transaction messages
    "Begin",
    "Continue",
    "End",
    "Abort",
    "Unidirectional",
    # components
    "Invoke",
    "ReturnResult",
    "ReturnError",
    "Reject",
    # operation / error codes
    "OperationCode",
    "ErrorCode",
    # codec
    "encode",
    "decode",
    "decode_detailed",
    "DecodeProblem",
    "TcapError",
    # dialogue portion (AARQ / AARE / ABRT / AUDT)
    "DialoguePdu",
    "dialogue_aarq",
    "dialogue_aare_accept",
    "dialogue_aare_reject",
    "dialogue_abrt",
    "dialogue_audt",
    "parse_dialogue_portion",
    "ABORT_SOURCE_USER",
    "ABORT_SOURCE_PROVIDER",
    # P-Abort causes (Q.773 Table 12)
    "P_ABORT_UNRECOGNIZED_MESSAGE_TYPE",
    "P_ABORT_UNRECOGNIZED_TRANSACTION_ID",
    "P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION",
    "P_ABORT_INCORRECT_TRANSACTION_PORTION",
    "P_ABORT_RESOURCE_LIMITATION",
    # Reject problem classes and general problems (Q.773 Tables 25, 26)
    "PROBLEM_GENERAL",
    "PROBLEM_INVOKE",
    "PROBLEM_RETURN_RESULT",
    "PROBLEM_RETURN_ERROR",
    "GENERAL_PROBLEM_UNRECOGNIZED_COMPONENT",
    "GENERAL_PROBLEM_MISTYPED_COMPONENT",
    "GENERAL_PROBLEM_BADLY_STRUCTURED_COMPONENT",
    # transaction PDU tags (Q.773)
    "TAG_UNIDIRECTIONAL",
    "TAG_BEGIN",
    "TAG_END",
    "TAG_CONTINUE",
    "TAG_ABORT",
    # component type numbers (Q.773 §3.2)
    "COMPONENT_INVOKE",
    "COMPONENT_RETURN_RESULT_LAST",
    "COMPONENT_RETURN_ERROR",
    "COMPONENT_REJECT",
    "COMPONENT_RETURN_RESULT_NOT_LAST",
    "__version__",
]
