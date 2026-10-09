"""Codec parity / round-trip tests for the tcap wheel.

These exercise the same Rust BER codec the crate ships, through the Python
surface: build a transaction with components, ``encode()`` to Q.773 wire bytes,
``decode()`` back, and check the fields survive. All values are synthetic /
spec-derived (fictional transaction ids, application-defined operation & error
codes, opaque parameter octets) — no captured traffic.
"""

from __future__ import annotations

import pytest

import tcap


def test_transaction_tag_constants() -> None:
    assert tcap.TAG_UNIDIRECTIONAL == 0x61
    assert tcap.TAG_BEGIN == 0x62
    assert tcap.TAG_END == 0x64
    assert tcap.TAG_CONTINUE == 0x65
    assert tcap.TAG_ABORT == 0x67


def test_component_type_constants() -> None:
    assert tcap.COMPONENT_INVOKE == 1
    assert tcap.COMPONENT_RETURN_RESULT_LAST == 2
    assert tcap.COMPONENT_RETURN_ERROR == 3
    assert tcap.COMPONENT_REJECT == 4
    assert tcap.COMPONENT_RETURN_RESULT_NOT_LAST == 7


# ── Operation / error codes ──────────────────────────────────────────────────
def test_operation_code_local() -> None:
    op = tcap.OperationCode.local(45)
    assert op.is_local
    assert op.value == 45
    assert op.oid is None


def test_operation_code_global() -> None:
    op = tcap.OperationCode.global_([0, 4, 0, 0, 1, 0, 21, 3])
    assert not op.is_local
    assert op.value is None
    assert op.oid == [0, 4, 0, 0, 1, 0, 21, 3]


def test_error_code_local() -> None:
    ec = tcap.ErrorCode.local(34)  # e.g. systemFailure
    assert ec.is_local
    assert ec.value == 34


# ── Begin + Invoke (opening leg) ─────────────────────────────────────────────
def test_begin_tag_is_application_2() -> None:
    wire = tcap.Begin(b"\x00\x00\x00\x01").encode()
    assert wire[0] == 0x62  # [APPLICATION 2] CONSTRUCTED


def test_begin_with_invoke_round_trip() -> None:
    invoke = tcap.Invoke(
        1,
        tcap.OperationCode.local(45),
        parameter=bytes([0x04, 0x03, 0x01, 0x02, 0x03]),  # synthetic OCTET STRING
    )
    begin = tcap.Begin(b"\x11\x22", components=[invoke])
    wire = begin.encode()
    assert wire[0] == 0x62

    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.Begin)
    assert decoded.otid == b"\x11\x22"
    assert len(decoded.components) == 1

    inv = decoded.components[0]
    assert isinstance(inv, tcap.Invoke)
    assert inv.invoke_id == 1
    assert inv.operation_code == tcap.OperationCode.local(45)
    assert inv.parameter == bytes([0x04, 0x03, 0x01, 0x02, 0x03])
    # re-encode reproduces the exact bytes
    assert decoded.encode() == wire


def test_begin_empty_round_trip() -> None:
    begin = tcap.Begin(b"\x00\x00\x00\x01")
    decoded = tcap.decode(begin.encode())
    assert isinstance(decoded, tcap.Begin)
    assert decoded.otid == b"\x00\x00\x00\x01"
    assert decoded.components == []


def test_begin_with_global_operation_code() -> None:
    arcs = [0, 4, 0, 0, 1, 0, 21, 3]  # synthetic OID, not a registered context
    invoke = tcap.Invoke(1, tcap.OperationCode.global_(arcs))
    begin = tcap.Begin(b"\x01", components=[invoke])
    decoded = tcap.decode(begin.encode())
    inv = decoded.components[0]
    assert inv.operation_code == tcap.OperationCode.global_(arcs)
    assert inv.operation_code.oid == arcs


# ── End + ReturnResult (closing leg) ─────────────────────────────────────────
def test_end_tag_is_application_4() -> None:
    wire = tcap.End(b"\x01").encode()
    assert wire[0] == 0x64  # [APPLICATION 4] CONSTRUCTED


def test_end_with_return_result_round_trip() -> None:
    rr = tcap.ReturnResult(
        1,
        operation_code=tcap.OperationCode.local(45),
        parameter=bytes([0x05, 0x00]),  # synthetic NULL
    )
    end = tcap.End(b"\x00\x00\x00\x02", components=[rr])
    wire = end.encode()
    assert wire[0] == 0x64

    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.End)
    assert decoded.dtid == b"\x00\x00\x00\x02"
    result = decoded.components[0]
    assert isinstance(result, tcap.ReturnResult)
    assert result.invoke_id == 1
    assert result.last is True
    assert result.operation_code == tcap.OperationCode.local(45)
    assert result.parameter == bytes([0x05, 0x00])
    assert decoded.encode() == wire


def test_return_result_not_last_preserved() -> None:
    rr = tcap.ReturnResult(
        2,
        operation_code=tcap.OperationCode.local(46),
        parameter=bytes([0x05, 0x00]),
        last=False,
    )
    cont = tcap.Continue(b"\xAA", b"\xBB", components=[rr])
    decoded = tcap.decode(cont.encode())
    result = decoded.components[0]
    assert isinstance(result, tcap.ReturnResult)
    assert result.last is False


def test_end_with_return_error_round_trip() -> None:
    re = tcap.ReturnError(5, tcap.ErrorCode.local(34))
    end = tcap.End(b"\x01", components=[re])
    decoded = tcap.decode(end.encode())
    err = decoded.components[0]
    assert isinstance(err, tcap.ReturnError)
    assert err.invoke_id == 5
    assert err.error_code == tcap.ErrorCode.local(34)


# ── Continue (mid-dialogue) ──────────────────────────────────────────────────
def test_continue_tag_and_round_trip() -> None:
    cont = tcap.Continue(b"\x01", b"\x02")
    wire = cont.encode()
    assert wire[0] == 0x65  # [APPLICATION 5] CONSTRUCTED
    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.Continue)
    assert decoded.otid == b"\x01"
    assert decoded.dtid == b"\x02"


# ── Abort ────────────────────────────────────────────────────────────────────
def test_abort_p_abort_round_trip() -> None:
    abort = tcap.Abort(b"\x03", p_abort_cause=tcap.P_ABORT_UNRECOGNIZED_TRANSACTION_ID)
    wire = abort.encode()
    # Abort [APPLICATION 7], dtid 03, P-AbortCause [APPLICATION 10] INTEGER 1.
    assert wire == bytes([0x67, 0x06, 0x49, 0x01, 0x03, 0x4A, 0x01, 0x01])
    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.Abort)
    assert decoded.dtid == b"\x03"
    assert decoded.p_abort_cause == 1
    assert decoded.dialogue_portion is None


def test_abort_u_abort_round_trip() -> None:
    abrt = tcap.dialogue_abrt(tcap.ABORT_SOURCE_USER)
    decoded = tcap.decode(tcap.Abort(b"\x03", dialogue_portion=abrt).encode())
    assert isinstance(decoded, tcap.Abort)
    assert decoded.p_abort_cause is None
    assert decoded.dialogue_portion == abrt
    pdu = tcap.parse_dialogue_portion(decoded.dialogue_portion)
    assert pdu is not None and pdu.pdu_type == "ABRT"


def test_abort_no_reason() -> None:
    decoded = tcap.decode(tcap.Abort(b"\x03").encode())
    assert isinstance(decoded, tcap.Abort)
    assert decoded.p_abort_cause is None
    assert decoded.dialogue_portion is None


def test_abort_takes_one_reason() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.Abort(b"\x03", p_abort_cause=0, dialogue_portion=tcap.dialogue_abrt(0))


# ── Unidirectional ───────────────────────────────────────────────────────────
def test_unidirectional_round_trip() -> None:
    invoke = tcap.Invoke(0, tcap.OperationCode.local(59))
    uni = tcap.Unidirectional(components=[invoke])
    wire = uni.encode()
    assert wire[0] == 0x61  # [APPLICATION 1] CONSTRUCTED
    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.Unidirectional)
    assert len(decoded.components) == 1


# ── Reject ───────────────────────────────────────────────────────────────────
def test_reject_round_trip() -> None:
    reject = tcap.Reject(
        99, tcap.PROBLEM_GENERAL, tcap.GENERAL_PROBLEM_MISTYPED_COMPONENT
    )
    end = tcap.End(b"\x01", components=[reject])
    wire = end.encode()
    # End, dtid 01, one reject [4] { INTEGER 99, generalProblem [0] 1 }.
    assert wire == bytes.fromhex("640d4901016c08a40602016380 0101".replace(" ", ""))
    rj = tcap.decode(wire).components[0]
    assert isinstance(rj, tcap.Reject)
    assert rj.invoke_id == 99
    assert rj.problem_type == tcap.PROBLEM_GENERAL
    assert rj.problem_code == tcap.GENERAL_PROBLEM_MISTYPED_COMPONENT


def test_reject_without_an_invoke_id() -> None:
    # The invoke ID could not be derived: a NULL on the wire.
    reject = tcap.Reject(None, tcap.PROBLEM_GENERAL, 0)
    wire = tcap.End(b"\x01", components=[reject]).encode()
    assert wire == bytes.fromhex("640c4901016c07a4050500800100")
    assert tcap.decode(wire).components[0].invoke_id is None


def test_reject_problem_type_is_checked() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.Reject(1, 4, 0)


def test_invoke_id_range() -> None:
    # InvokeIdType ::= INTEGER (-128..127)
    with pytest.raises(OverflowError):
        tcap.Invoke(128, tcap.OperationCode.local(1))
    assert tcap.Invoke(-128, tcap.OperationCode.local(1)).invoke_id == -128


# ── Multiple components ──────────────────────────────────────────────────────
def test_multiple_components() -> None:
    begin = tcap.Begin(
        b"\x01",
        components=[
            tcap.Invoke(1, tcap.OperationCode.local(45)),
            tcap.Invoke(2, tcap.OperationCode.local(46)),
        ],
    )
    decoded = tcap.decode(begin.encode())
    assert [c.invoke_id for c in decoded.components] == [1, 2]


# ── module-level encode() dispatch ───────────────────────────────────────────
def test_module_encode_matches_method() -> None:
    begin = tcap.Begin(b"\x01", components=[tcap.Invoke(1, tcap.OperationCode.local(45))])
    assert tcap.encode(begin) == begin.encode()


# ── dialogue portion ─────────────────────────────────────────────────────────
def test_dialogue_portion_round_trip() -> None:
    # A synthetic EXTERNAL (tag 0x28 = [UNIVERSAL 8]) in the made-up abstract
    # syntax 2.999.1, holding an OCTET STRING as a single-ASN1-type.
    dp = bytes.fromhex("280a0603883701a003040101")
    begin = tcap.Begin(b"\x01", dialogue_portion=dp)
    decoded = tcap.decode(begin.encode())
    assert decoded.dialogue_portion == dp


# ── Error paths ──────────────────────────────────────────────────────────────
def test_decode_garbage_raises() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.decode(bytes([0xFF, 0x00, 0x99]))


def test_decode_empty_raises() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.decode(b"")


def test_decode_truncated_raises() -> None:
    # 0x62 = Begin, 0x7F = length 127, but no content follows.
    with pytest.raises(tcap.TcapError):
        tcap.decode(bytes([0x62, 0x7F]))


def test_encode_rejects_non_message() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.encode(tcap.Invoke(1, tcap.OperationCode.local(45)))  # a component, not a message


# ── Dialogue portion (AARQ / AARE / ABRT) ─────────────────────────────────────

# MAP shortMsgGateway v3 — a real, registered application context (SRI-SM).
MAP_SRI_SM_AC = [0, 4, 0, 0, 1, 0, 20, 3]
# CAP gsmSSF-scfGeneric v3 — the CAMEL call application context.
CAP_GSMSSF_SCF_AC = [0, 4, 0, 0, 1, 21, 3, 4]


def test_dialogue_aarq_starts_at_external_tag() -> None:
    dp = tcap.dialogue_aarq(MAP_SRI_SM_AC)
    # The EXTERNAL tag 0x28, NOT the outer [APPLICATION 11] 0x6B (tcap adds that).
    assert dp[0] == 0x28
    # AARQ tag 0x60 appears inside.
    assert 0x60 in dp


def test_dialogue_aarq_round_trips_through_begin() -> None:
    dp = tcap.dialogue_aarq(MAP_SRI_SM_AC)
    begin = tcap.Begin(b"\x00\x00\x00\x01", dialogue_portion=dp)
    decoded = tcap.decode(begin.encode())
    assert decoded.dialogue_portion == dp
    pdu = tcap.parse_dialogue_portion(decoded.dialogue_portion)
    assert pdu is not None
    assert pdu.pdu_type == "AARQ"
    assert pdu.application_context == MAP_SRI_SM_AC
    assert pdu.result is None
    assert pdu.abort_source is None


def test_dialogue_aare_accept_read_back() -> None:
    dp = tcap.dialogue_aare_accept(CAP_GSMSSF_SCF_AC)
    end = tcap.End(b"\x00\x00\x00\x02", dialogue_portion=dp)
    decoded = tcap.decode(end.encode())
    pdu = tcap.parse_dialogue_portion(decoded.dialogue_portion)
    assert pdu is not None
    assert pdu.pdu_type == "AARE"
    assert pdu.application_context == CAP_GSMSSF_SCF_AC
    assert pdu.result == 0  # accepted
    assert pdu.result_source_diagnostic == (1, 0)  # dialogue-service-user null(0)


def test_dialogue_abrt_round_trip() -> None:
    dp = tcap.dialogue_abrt(tcap.ABORT_SOURCE_PROVIDER)
    pdu = tcap.parse_dialogue_portion(dp)
    assert pdu is not None
    assert pdu.pdu_type == "ABRT"
    assert pdu.abort_source == tcap.ABORT_SOURCE_PROVIDER
    assert pdu.application_context is None


def test_dialogue_abrt_source_constants() -> None:
    assert tcap.ABORT_SOURCE_USER == 0
    assert tcap.ABORT_SOURCE_PROVIDER == 1


def test_dialogue_abrt_rejects_bad_source() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.dialogue_abrt(7)


def test_dialogue_aarq_rejects_bad_oid() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.dialogue_aarq([3, 0, 0])  # first arc > 2 is not a valid OID


def test_parse_user_defined_syntax_returns_none() -> None:
    # A well-formed EXTERNAL in the made-up abstract syntax 2.999.1, holding an
    # OCTET STRING: not a dialogue PDU, and not an error.
    external = bytes.fromhex("280a0603883701a003040101")
    assert tcap.parse_dialogue_portion(external) is None


def test_parse_malformed_dialogue_raises() -> None:
    # An EXTERNAL without its encoding member.
    with pytest.raises(tcap.TcapError):
        tcap.parse_dialogue_portion(bytes([0x28, 0x03, 0x06, 0x01, 0x2A]))
    # An AARQ holding an INTEGER where the application context name belongs.
    with pytest.raises(tcap.TcapError):
        tcap.parse_dialogue_portion(
            bytes.fromhex("2816060700118605010101a00b600980020780a103020105")
        )


def test_dialogue_aare_reject_and_audt() -> None:
    pdu = tcap.parse_dialogue_portion(tcap.dialogue_aare_reject(MAP_SRI_SM_AC, 2, 2))
    assert pdu is not None
    assert (pdu.pdu_type, pdu.result, pdu.result_source_diagnostic) == ("AARE", 1, (2, 2))
    pdu = tcap.parse_dialogue_portion(tcap.dialogue_audt(MAP_SRI_SM_AC))
    assert pdu is not None
    assert (pdu.pdu_type, pdu.version1) == ("AUDT", True)


# ── A message that is not fully understood ───────────────────────────────────
# A Begin whose second Invoke stops after its invoke ID.
DROPPED_LAST = bytes.fromhex("62154804000010016c0da10602010102013aa103020102")


def test_decode_raises_with_the_problem_attached() -> None:
    with pytest.raises(tcap.TcapError) as caught:
        tcap.decode(DROPPED_LAST)
    problem = caught.value.problem
    assert isinstance(problem, tcap.DecodeProblem)
    assert problem.sublayer == "component"
    assert problem.fault == "component"
    assert problem.component_index == 1
    assert problem.component_type == tcap.COMPONENT_INVOKE
    assert problem.invoke_id == 2
    assert problem.general_problem == tcap.GENERAL_PROBLEM_MISTYPED_COMPONENT
    assert problem.message_type == tcap.TAG_BEGIN
    assert problem.otid == bytes.fromhex("00001001")
    # The component before the faulty one stands.
    assert [c.invoke_id for c in problem.partial.components] == [1]
    reject = problem.reject()
    assert (reject.invoke_id, reject.problem_type, reject.problem_code) == (2, 0, 1)
    assert problem.abort() is None


def test_decode_detailed_returns_the_problem() -> None:
    # Octets after the end of the message.
    wire = tcap.Begin(b"\x00\x00\x10\x01").encode() + b"\x00"
    problem = tcap.decode_detailed(wire)
    assert isinstance(problem, tcap.DecodeProblem)
    assert problem.sublayer == "transaction"
    assert problem.p_abort_cause == tcap.P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION
    assert problem.partial is None and problem.reject() is None
    abort = problem.abort()
    assert abort.dtid == bytes.fromhex("00001001")
    assert abort.encode() == bytes.fromhex("67094904000010014a0102")


def test_decode_detailed_returns_the_message() -> None:
    wire = tcap.Begin(b"\x01").encode()
    assert isinstance(tcap.decode_detailed(wire), tcap.Begin)


def test_malformed_dialogue_portion_is_answered_with_an_abrt() -> None:
    wire = bytes.fromhex(
        "62204804000010016b182816060700118605010101a00b600980020780a103020105"
    )
    problem = tcap.decode_detailed(wire)
    assert problem.fault == "dialogue_portion"
    abort = problem.abort()
    assert abort.dialogue_portion == tcap.dialogue_abrt(tcap.ABORT_SOURCE_PROVIDER)


def test_transaction_id_length_is_checked() -> None:
    with pytest.raises(tcap.TcapError):
        tcap.Begin(b"\x01\x02\x03\x04\x05").encode()
    with pytest.raises(tcap.TcapError):
        tcap.Begin(b"").encode()
