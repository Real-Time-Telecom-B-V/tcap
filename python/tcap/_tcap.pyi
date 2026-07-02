"""Type stubs for the Rust-backed ``tcap._tcap`` extension module.

TCAP messages carry application-decoded content (operation arguments, the
dialogue ``EXTERNAL``, a ``Reject`` problem) as opaque BER — those fields are
``bytes`` here, exactly as the Rust codec keeps them. Transaction ids
(OTID/DTID) are ``bytes``; invoke ids and local operation/error codes are
``int``.
"""

from __future__ import annotations

from typing import Sequence

# ── Q.773 transaction PDU tags (first BER byte of an encoded message) ─────────
TAG_UNIDIRECTIONAL: int
TAG_BEGIN: int
TAG_END: int
TAG_CONTINUE: int
TAG_ABORT: int

# ── Component type numbers (Q.773 §3.2, CONTEXT class) ────────────────────────
COMPONENT_INVOKE: int
COMPONENT_RETURN_RESULT_LAST: int
COMPONENT_RETURN_ERROR: int
COMPONENT_REJECT: int
COMPONENT_RETURN_RESULT_NOT_LAST: int

class TcapError(Exception):
    """TCAP protocol / codec error (ITU-T Q.771–Q.775)."""

class OperationCode:
    """A TCAP operation code — a ``local`` integer or a ``global`` OID."""

    @staticmethod
    def local(value: int) -> OperationCode: ...
    @staticmethod
    def global_(arcs: Sequence[int]) -> OperationCode: ...
    @property
    def is_local(self) -> bool: ...
    @property
    def value(self) -> int | None:
        """The local integer value, or ``None`` for a global code."""
    @property
    def oid(self) -> list[int] | None:
        """The OID arcs, or ``None`` for a local code."""
    def __eq__(self, other: object) -> bool: ...

class ErrorCode:
    """A TCAP error code — a ``local`` integer or a ``global`` OID."""

    @staticmethod
    def local(value: int) -> ErrorCode: ...
    @staticmethod
    def global_(arcs: Sequence[int]) -> ErrorCode: ...
    @property
    def is_local(self) -> bool: ...
    @property
    def value(self) -> int | None: ...
    @property
    def oid(self) -> list[int] | None: ...
    def __eq__(self, other: object) -> bool: ...

# ── Components ────────────────────────────────────────────────────────────────
class Invoke:
    """A TCAP Invoke component — carries an operation for the peer."""

    invoke_id: int
    linked_id: int | None
    operation_code: OperationCode
    parameter: bytes | None
    def __init__(
        self,
        invoke_id: int,
        operation_code: OperationCode,
        *,
        linked_id: int | None = None,
        parameter: bytes | None = None,
    ) -> None: ...

class ReturnResult:
    """A TCAP ReturnResult component — the successful result of an Invoke.

    ``last=True`` (default) encodes ReturnResultLast ``[CONTEXT 2]``;
    ``last=False`` encodes ReturnResultNotLast ``[CONTEXT 7]``.
    """

    invoke_id: int
    last: bool
    operation_code: OperationCode | None
    parameter: bytes | None
    def __init__(
        self,
        invoke_id: int,
        *,
        operation_code: OperationCode | None = None,
        parameter: bytes | None = None,
        last: bool = True,
    ) -> None: ...

class ReturnError:
    """A TCAP ReturnError component — a failure response to an Invoke."""

    invoke_id: int
    error_code: ErrorCode
    parameter: bytes | None
    def __init__(
        self,
        invoke_id: int,
        error_code: ErrorCode,
        *,
        parameter: bytes | None = None,
    ) -> None: ...

class Reject:
    """A TCAP Reject component — rejects a received component."""

    invoke_id: int
    problem: bytes
    def __init__(self, invoke_id: int, problem: bytes) -> None: ...

Component = Invoke | ReturnResult | ReturnError | Reject

# ── Transaction messages ──────────────────────────────────────────────────────
class Begin:
    """A TCAP Begin transaction (``[APPLICATION 2]``) — opens a dialogue."""

    otid: bytes
    dialogue_portion: bytes | None
    components: list[Component]
    def __init__(
        self,
        otid: bytes,
        *,
        components: Sequence[Component] | None = None,
        dialogue_portion: bytes | None = None,
    ) -> None: ...
    def encode(self) -> bytes:
        """Encode this Begin to Q.773-compliant BER bytes."""

class Continue:
    """A TCAP Continue transaction (``[APPLICATION 5]``) — mid-dialogue."""

    otid: bytes
    dtid: bytes
    dialogue_portion: bytes | None
    components: list[Component]
    def __init__(
        self,
        otid: bytes,
        dtid: bytes,
        *,
        components: Sequence[Component] | None = None,
        dialogue_portion: bytes | None = None,
    ) -> None: ...
    def encode(self) -> bytes: ...

class End:
    """A TCAP End transaction (``[APPLICATION 4]``) — closes a dialogue."""

    dtid: bytes
    dialogue_portion: bytes | None
    components: list[Component]
    def __init__(
        self,
        dtid: bytes,
        *,
        components: Sequence[Component] | None = None,
        dialogue_portion: bytes | None = None,
    ) -> None: ...
    def encode(self) -> bytes: ...

class Abort:
    """A TCAP Abort transaction (``[APPLICATION 7]``) — aborts a dialogue."""

    dtid: bytes
    reason: bytes | None
    def __init__(self, dtid: bytes, *, reason: bytes | None = None) -> None: ...
    def encode(self) -> bytes: ...

class Unidirectional:
    """A TCAP Unidirectional transaction (``[APPLICATION 1]``) — fire-and-forget."""

    dialogue_portion: bytes | None
    components: list[Component]
    def __init__(
        self,
        *,
        components: Sequence[Component] | None = None,
        dialogue_portion: bytes | None = None,
    ) -> None: ...
    def encode(self) -> bytes: ...

Message = Begin | Continue | End | Abort | Unidirectional

def encode(message: Message) -> bytes:
    """Encode any TCAP message to BER bytes (same as ``message.encode()``)."""

def decode(data: bytes) -> Message:
    """Decode a TCAP message from BER bytes into the matching message class."""
