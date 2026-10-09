"""Type stubs for the Rust-backed ``tcap._tcap`` extension module.

TCAP messages carry application-decoded content (operation arguments, the
dialogue ``EXTERNAL``) as opaque BER — those fields are ``bytes`` here, exactly
as the Rust codec keeps them. Transaction ids (OTID/DTID) are ``bytes`` of 1 to
4 octets; invoke ids (-128..127), local operation/error codes, the P-Abort
cause and the Reject problem class and code are ``int``.
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

# ── ABRT-source values (Q.773 §3.2.1) ─────────────────────────────────────────
ABORT_SOURCE_USER: int
ABORT_SOURCE_PROVIDER: int

# ── P-Abort causes (Q.773 Table 12) ───────────────────────────────────────────
P_ABORT_UNRECOGNIZED_MESSAGE_TYPE: int
P_ABORT_UNRECOGNIZED_TRANSACTION_ID: int
P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION: int
P_ABORT_INCORRECT_TRANSACTION_PORTION: int
P_ABORT_RESOURCE_LIMITATION: int

# ── Reject problem classes (Q.773 Table 25) and general problems (Table 26) ───
PROBLEM_GENERAL: int
PROBLEM_INVOKE: int
PROBLEM_RETURN_RESULT: int
PROBLEM_RETURN_ERROR: int
GENERAL_PROBLEM_UNRECOGNIZED_COMPONENT: int
GENERAL_PROBLEM_MISTYPED_COMPONENT: int
GENERAL_PROBLEM_BADLY_STRUCTURED_COMPONENT: int

class TcapError(Exception):
    """TCAP protocol / codec error (ITU-T Q.771–Q.775).

    When raised by ``decode`` for a message that was not fully understood, the
    ``problem`` attribute holds the ``DecodeProblem``.
    """

    problem: DecodeProblem

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
    """A TCAP Reject component — rejects a received component.

    ``invoke_id`` is ``None`` when the invoke ID of the rejected component could
    not be derived (sent as a NULL). ``problem_type`` is the problem class
    (``PROBLEM_*``) and ``problem_code`` its value (Q.773 Tables 26 to 29).
    """

    invoke_id: int | None
    problem_type: int
    problem_code: int
    def __init__(self, invoke_id: int | None, problem_type: int, problem_code: int) -> None: ...

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
    p_abort_cause: int | None
    dialogue_portion: bytes | None
    def __init__(
        self,
        dtid: bytes,
        *,
        p_abort_cause: int | None = None,
        dialogue_portion: bytes | None = None,
    ) -> None: ...
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
    """Decode a TCAP message from BER bytes into the matching message class.

    The message has to be fully valid: when any part of it was not understood,
    ``TcapError`` is raised with the ``DecodeProblem`` as its ``problem``
    attribute. A message is never returned without something that was on the
    wire.
    """

def decode_detailed(data: bytes) -> Message | DecodeProblem:
    """Decode without raising: the message, or a ``DecodeProblem``."""

class DecodeProblem:
    """A message that was not fully understood, with what Q.774 needs to answer it."""

    @property
    def sublayer(self) -> str:
        """``"transaction"`` or ``"component"``: the sub-layer that detected it."""
    @property
    def fault(self) -> str:
        """``"transaction_portion"``, ``"dialogue_portion"`` or ``"component"``."""
    @property
    def p_abort_cause(self) -> int | None:
        """The P-Abort cause, for a transaction portion fault."""
    @property
    def general_problem(self) -> int | None:
        """The general problem code, for a component fault."""
    @property
    def component_index(self) -> int | None: ...
    @property
    def component_type(self) -> int | None:
        """``COMPONENT_*`` of the faulty component, ``None`` if not recognized."""
    @property
    def invoke_id(self) -> int | None:
        """The invoke ID of the faulty component, ``None`` if not derivable."""
    @property
    def message_type(self) -> int | None:
        """``TAG_*`` of the message, ``None`` if not recognized."""
    @property
    def otid(self) -> bytes | None: ...
    @property
    def dtid(self) -> bytes | None: ...
    @property
    def partial(self) -> Message | None:
        """For a component fault: the message with the components before it."""
    @property
    def detail(self) -> str: ...
    def abort(self) -> Abort | None:
        """The Abort to send to the originator, or ``None``."""
    def reject(self) -> Reject | None:
        """The Reject component to send, or ``None``."""

# ── Dialogue portion (AARQ / AARE / ABRT) ─────────────────────────────────────
class DialoguePdu:
    """A decoded dialogue PDU read back from a message's ``dialogue_portion``.

    Inspect ``pdu_type`` (``"AARQ"`` / ``"AARE"`` / ``"ABRT"`` / ``"AUDT"``);
    AARQ/AARE/AUDT expose ``application_context`` (OID arcs) and ``version1``,
    AARE also ``result`` and ``result_source_diagnostic``; ABRT exposes
    ``abort_source``.
    """

    @property
    def pdu_type(self) -> str: ...
    @property
    def application_context(self) -> list[int] | None:
        """The application-context-name OID arcs (AARQ / AARE / AUDT), else ``None``."""
    @property
    def version1(self) -> bool | None:
        """Whether the protocol version lists version 1 (AARQ / AARE / AUDT)."""
    @property
    def result(self) -> int | None:
        """AARE associate result: 0 accepted, 1 reject-permanent."""
    @property
    def result_source_diagnostic(self) -> tuple[int, int] | None:
        """AARE ``(source, value)``; source 1 = user, 2 = provider."""
    @property
    def abort_source(self) -> int | None:
        """ABRT source: 0 = user, 1 = provider."""
    @property
    def user_information(self) -> bytes | None:
        """The user-information ``[30]`` content octets (its EXTERNALs), if present."""

def dialogue_aarq(application_context: Sequence[int]) -> bytes:
    """Build an AARQ dialogue portion (``EXTERNAL`` bytes) for the given OID arcs."""

def dialogue_aare_accept(application_context: Sequence[int]) -> bytes:
    """Build an accepting AARE dialogue portion (``EXTERNAL`` bytes) for the OID arcs."""

def dialogue_aare_reject(application_context: Sequence[int], source: int, value: int) -> bytes:
    """Build a refusing AARE (reject-permanent) with diagnostic ``(source, value)``."""

def dialogue_abrt(abort_source: int) -> bytes:
    """Build an ABRT dialogue portion for ``abort_source`` (0 = user, 1 = provider)."""

def dialogue_audt(application_context: Sequence[int]) -> bytes:
    """Build an AUDT dialogue portion (``EXTERNAL`` bytes), for a Unidirectional."""

def parse_dialogue_portion(data: bytes) -> DialoguePdu | None:
    """Parse a dialogue portion's ``EXTERNAL`` bytes.

    A ``DialoguePdu`` for an AARQ / AARE / ABRT / AUDT; ``None`` for a
    well-formed ``EXTERNAL`` carrying something else; raises ``TcapError`` when
    the portion is malformed.
    """
