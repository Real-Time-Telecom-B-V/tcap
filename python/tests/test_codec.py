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
    rr = tcap.ReturnResult(2, operation_code=tcap.OperationCode.local(46), last=False)
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
def test_abort_round_trip() -> None:
    abort = tcap.Abort(b"\x03", reason=bytes([0x0A, 0x01, 0x01]))  # synthetic P-Abort
    wire = abort.encode()
    assert wire[0] == 0x67  # [APPLICATION 7] CONSTRUCTED
    decoded = tcap.decode(wire)
    assert isinstance(decoded, tcap.Abort)
    assert decoded.dtid == b"\x03"
    assert decoded.reason == bytes([0x0A, 0x01, 0x01])


def test_abort_no_reason() -> None:
    decoded = tcap.decode(tcap.Abort(b"\x03").encode())
    assert isinstance(decoded, tcap.Abort)
    assert decoded.reason is None


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
    # general problem: unrecognized component (synthetic problem BER)
    reject = tcap.Reject(99, bytes([0x80, 0x01, 0x01]))
    end = tcap.End(b"\x01", components=[reject])
    decoded = tcap.decode(end.encode())
    rj = decoded.components[0]
    assert isinstance(rj, tcap.Reject)
    assert rj.invoke_id == 99
    assert rj.problem == bytes([0x80, 0x01, 0x01])


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
    # Synthetic EXTERNAL-shaped bytes (tag 0x28 = [UNIVERSAL 8] EXTERNAL).
    dp = bytes([0x28, 0x03, 0x06, 0x01, 0x2A])
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


def test_parse_non_dialogue_returns_none() -> None:
    # A well-formed EXTERNAL but not the dialogue-as OID — not a dialogue PDU.
    assert tcap.parse_dialogue_portion(bytes([0x28, 0x03, 0x06, 0x01, 0x2A])) is None
