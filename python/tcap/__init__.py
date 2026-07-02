"""tcap — Rust-backed TCAP (ITU-T Q.771–Q.775) BER codec for Python.

TCAP (Transaction Capabilities Application Part) is the SS7 transaction-and-
component layer that carries MAP, CAP, and INAP dialogues. This package exposes
the same BER codec the Rust crate (``cargo add tcap``) ships, from one source
tree / one version: build a transaction (``Begin`` / ``Continue`` / ``End`` /
``Abort`` / ``Unidirectional``) with its components (``Invoke`` /
``ReturnResult`` / ``ReturnError`` / ``Reject``), call ``.encode()`` for wire
bytes, and ``tcap.decode(bytes)`` to parse.

The wire work (BER encode/decode, tag handling) runs in Rust; Python just builds
and inspects messages. Operation arguments, the dialogue ``EXTERNAL``, and a
``Reject`` problem are carried opaquely as ``bytes`` — the application layer
above (e.g. a MAP stack) decodes them.

Note: the PyPI distribution is ``ss7-tcap`` (``tcap`` is taken), but the import
name is ``tcap``.
"""

from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version

from ._tcap import (
    COMPONENT_INVOKE,
    COMPONENT_REJECT,
    COMPONENT_RETURN_ERROR,
    COMPONENT_RETURN_RESULT_LAST,
    COMPONENT_RETURN_RESULT_NOT_LAST,
    TAG_ABORT,
    TAG_BEGIN,
    TAG_CONTINUE,
    TAG_END,
    TAG_UNIDIRECTIONAL,
    Abort,
    Begin,
    Continue,
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
    encode,
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
    "TcapError",
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
