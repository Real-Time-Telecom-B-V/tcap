#!/usr/bin/env python3
"""Encode TCAP messages with pyasn1, an encoder this crate shares no code with.

The tests need bytes the crate did not produce. This script transcribes the
ASN.1 of ITU-T Q.773 (06/97) clauses 3.1 and 3.2 into pyasn1 types and prints
one line of hex per message. The output is pasted into
tests/foreign_vectors.rs, where each vector must decode to the value named
there. Rerun after changing a vector:

    python3 -I scripts/foreign_vectors.py

Each message is printed twice: with definite lengths, and with the indefinite
form on every constructed element, which 4.1.2.3/Q.773 allows and this crate
never emits.

All values are synthetic.
"""

from pyasn1.codec.ber import encoder
from pyasn1.type import char, constraint, namedtype, tag, univ


def implicit(cls, number):
    return tag.Tag(cls, tag.tagFormatSimple, number)


def app(number, constructed=False):
    fmt = tag.tagFormatConstructed if constructed else tag.tagFormatSimple
    return tag.Tag(tag.tagClassApplication, fmt, number)


def ctx(number, constructed=False):
    fmt = tag.tagFormatConstructed if constructed else tag.tagFormatSimple
    return tag.Tag(tag.tagClassContext, fmt, number)


# -- X.690 8.18.1: EXTERNAL ---------------------------------------------------
class Encoding(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("single-ASN1-type", univ.Any().subtype(explicitTag=ctx(0, True))),
        namedtype.NamedType("octet-aligned", univ.OctetString().subtype(implicitTag=ctx(1))),
        namedtype.NamedType("arbitrary", univ.BitString().subtype(implicitTag=ctx(2))),
    )


class External(univ.Sequence):
    tagSet = univ.Sequence.tagSet.tagImplicitly(
        tag.Tag(tag.tagClassUniversal, tag.tagFormatConstructed, 8)
    )
    componentType = namedtype.NamedTypes(
        namedtype.OptionalNamedType("direct-reference", univ.ObjectIdentifier()),
        namedtype.OptionalNamedType("indirect-reference", univ.Integer()),
        namedtype.OptionalNamedType("data-value-descriptor", char.GraphicString()),
        namedtype.NamedType("encoding", Encoding()),
    )


class UserInformation(univ.SequenceOf):
    componentType = External()


# -- Q.773 3.2: dialogue PDUs --------------------------------------------------
class AARQ(univ.Sequence):
    tagSet = univ.Sequence.tagSet.tagImplicitly(app(0, True))
    componentType = namedtype.NamedTypes(
        namedtype.OptionalNamedType(
            "protocol-version", univ.BitString().subtype(implicitTag=ctx(0))
        ),
        namedtype.NamedType(
            "application-context-name",
            univ.ObjectIdentifier().subtype(explicitTag=ctx(1, True)),
        ),
        namedtype.OptionalNamedType(
            "user-information", UserInformation().subtype(implicitTag=ctx(30, True))
        ),
    )


class SourceDiagnostic(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType(
            "dialogue-service-user", univ.Integer().subtype(explicitTag=ctx(1, True))
        ),
        namedtype.NamedType(
            "dialogue-service-provider", univ.Integer().subtype(explicitTag=ctx(2, True))
        ),
    )


class AARE(univ.Sequence):
    tagSet = univ.Sequence.tagSet.tagImplicitly(app(1, True))
    componentType = namedtype.NamedTypes(
        namedtype.OptionalNamedType(
            "protocol-version", univ.BitString().subtype(implicitTag=ctx(0))
        ),
        namedtype.NamedType(
            "application-context-name",
            univ.ObjectIdentifier().subtype(explicitTag=ctx(1, True)),
        ),
        namedtype.NamedType("result", univ.Integer().subtype(explicitTag=ctx(2, True))),
        namedtype.NamedType(
            "result-source-diagnostic", SourceDiagnostic().subtype(explicitTag=ctx(3, True))
        ),
        namedtype.OptionalNamedType(
            "user-information", UserInformation().subtype(implicitTag=ctx(30, True))
        ),
    )


class ABRT(univ.Sequence):
    tagSet = univ.Sequence.tagSet.tagImplicitly(app(4, True))
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("abort-source", univ.Integer().subtype(implicitTag=ctx(0))),
        namedtype.OptionalNamedType(
            "user-information", UserInformation().subtype(implicitTag=ctx(30, True))
        ),
    )


# -- Q.773 3.1: components -----------------------------------------------------
class Operation(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("localValue", univ.Integer()),
        namedtype.NamedType("globalValue", univ.ObjectIdentifier()),
    )


class Invoke(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("invokeID", univ.Integer()),
        namedtype.OptionalNamedType("linkedID", univ.Integer().subtype(implicitTag=ctx(0))),
        namedtype.NamedType("operationCode", Operation()),
        namedtype.OptionalNamedType("parameter", univ.Any()),
    )


class Result(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("operationCode", Operation()),
        namedtype.NamedType("parameter", univ.Any()),
    )


class ReturnResult(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("invokeID", univ.Integer()),
        namedtype.OptionalNamedType("result", Result()),
    )


class ReturnError(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("invokeID", univ.Integer()),
        namedtype.NamedType("errorCode", Operation()),
        namedtype.OptionalNamedType("parameter", univ.Any()),
    )


class RejectInvokeId(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("derivable", univ.Integer()),
        namedtype.NamedType("not-derivable", univ.Null()),
    )


class Problem(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("generalProblem", univ.Integer().subtype(implicitTag=ctx(0))),
        namedtype.NamedType("invokeProblem", univ.Integer().subtype(implicitTag=ctx(1))),
        namedtype.NamedType("returnResultProblem", univ.Integer().subtype(implicitTag=ctx(2))),
        namedtype.NamedType("returnErrorProblem", univ.Integer().subtype(implicitTag=ctx(3))),
    )


class Reject(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("invokeID", RejectInvokeId()),
        namedtype.NamedType("problem", Problem()),
    )


class Component(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("invoke", Invoke().subtype(implicitTag=ctx(1, True))),
        namedtype.NamedType("returnResultLast", ReturnResult().subtype(implicitTag=ctx(2, True))),
        namedtype.NamedType("returnError", ReturnError().subtype(implicitTag=ctx(3, True))),
        namedtype.NamedType("reject", Reject().subtype(implicitTag=ctx(4, True))),
        namedtype.NamedType(
            "returnResultNotLast", ReturnResult().subtype(implicitTag=ctx(7, True))
        ),
    )


class ComponentPortion(univ.SequenceOf):
    tagSet = univ.SequenceOf.tagSet.tagImplicitly(app(12, True))
    componentType = Component()
    subtypeSpec = constraint.ValueSizeConstraint(1, 64)


# -- Q.773 3.1: messages --------------------------------------------------------
def otid():
    return univ.OctetString().subtype(implicitTag=app(8))


def dtid():
    return univ.OctetString().subtype(implicitTag=app(9))


def dialogue_portion():
    # DialoguePortion ::= [APPLICATION 11] EXTERNAL, an explicit tag.
    return External().subtype(explicitTag=app(11, True))


class AbortReason(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("p-abortCause", univ.Integer().subtype(implicitTag=app(10))),
        namedtype.NamedType("u-abortCause", dialogue_portion()),
    )


class Unidirectional(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.OptionalNamedType("dialoguePortion", dialogue_portion()),
        namedtype.NamedType("components", ComponentPortion()),
    )


class Begin(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("otid", otid()),
        namedtype.OptionalNamedType("dialoguePortion", dialogue_portion()),
        namedtype.OptionalNamedType("components", ComponentPortion()),
    )


class End(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("dtid", dtid()),
        namedtype.OptionalNamedType("dialoguePortion", dialogue_portion()),
        namedtype.OptionalNamedType("components", ComponentPortion()),
    )


class Continue(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("otid", otid()),
        namedtype.NamedType("dtid", dtid()),
        namedtype.OptionalNamedType("dialoguePortion", dialogue_portion()),
        namedtype.OptionalNamedType("components", ComponentPortion()),
    )


class Abort(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("dtid", dtid()),
        namedtype.OptionalNamedType("reason", AbortReason()),
    )


class Message(univ.Choice):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("unidirectional", Unidirectional().subtype(implicitTag=app(1, True))),
        namedtype.NamedType("begin", Begin().subtype(implicitTag=app(2, True))),
        namedtype.NamedType("end", End().subtype(implicitTag=app(4, True))),
        namedtype.NamedType("continue", Continue().subtype(implicitTag=app(5, True))),
        namedtype.NamedType("abort", Abort().subtype(implicitTag=app(7, True))),
    )


# -- Values ----------------------------------------------------------------------
DIALOGUE_AS = "0.0.17.773.1.1.1"
UNIDIALOGUE_AS = "0.0.17.773.1.2.1"
# imsiRetrievalContext-v2 of 3GPP TS 29.002.
CONTEXT = "0.4.0.0.1.0.26.2"
# A made-up abstract syntax for user information, under the example arc
# {joint-iso-itu-t(2) example(999)} of X.660.
USER_SYNTAX = "2.999.1"

# Operation 58 argument and result of the same context, as octet strings:
# a number in the fictional +1 555 01xx range, an identity in the test network
# 001 01.
NUMBER = bytes.fromhex("0407915155101032f4")
IDENTITY = bytes.fromhex("040800010121436587f9")


def external(syntax, pdu):
    value = External()
    value["direct-reference"] = syntax
    value["encoding"]["single-ASN1-type"] = encoder.encode(pdu)
    return value


def user_information(pdu):
    info = pdu["user-information"]
    item = External()
    item["direct-reference"] = USER_SYNTAX
    item["encoding"]["single-ASN1-type"] = bytes.fromhex("040101")
    info.append(item)


def set_dialogue(target, name, syntax, pdu):
    portion = target[name]
    portion["direct-reference"] = syntax
    portion["encoding"]["single-ASN1-type"] = encoder.encode(pdu)


def invoke(invoke_id, operation, parameter=None, linked=None):
    component = Component()
    body = component["invoke"]
    body["invokeID"] = invoke_id
    if linked is not None:
        body["linkedID"] = linked
    if isinstance(operation, str):
        body["operationCode"]["globalValue"] = operation
    else:
        body["operationCode"]["localValue"] = operation
    if parameter is not None:
        body["parameter"] = parameter
    return component


def result(invoke_id, operation=None, parameter=None, last=True):
    component = Component()
    body = component["returnResultLast" if last else "returnResultNotLast"]
    body["invokeID"] = invoke_id
    if operation is not None:
        body["result"]["operationCode"]["localValue"] = operation
        body["result"]["parameter"] = parameter
    return component


def error(invoke_id, code, parameter=None):
    component = Component()
    body = component["returnError"]
    body["invokeID"] = invoke_id
    if isinstance(code, str):
        body["errorCode"]["globalValue"] = code
    else:
        body["errorCode"]["localValue"] = code
    if parameter is not None:
        body["parameter"] = parameter
    return component


def reject(invoke_id, problem, value):
    component = Component()
    body = component["reject"]
    if invoke_id is None:
        body["invokeID"]["not-derivable"] = univ.Null("")
    else:
        body["invokeID"]["derivable"] = invoke_id
    body["problem"][problem] = value
    return component


def message(kind, otid_value=None, dtid_value=None, components=None):
    value = Message()
    body = value[kind]
    if otid_value is not None:
        body["otid"] = otid_value
    if dtid_value is not None:
        body["dtid"] = dtid_value
    for component in components or []:
        body["components"].append(component)
    return value


def vectors():
    # Begin: AARQ with user information, one Invoke with a parameter.
    begin = message("begin", bytes.fromhex("00001001"), None, [invoke(1, 58, NUMBER)])
    aarq = AARQ()
    aarq["protocol-version"] = univ.BitString("'1'B").subtype(implicitTag=ctx(0))
    aarq["application-context-name"] = CONTEXT
    user_information(aarq)
    set_dialogue(begin["begin"], "dialoguePortion", DIALOGUE_AS, aarq)
    yield "begin_aarq_invoke", begin

    # Continue: AARE accepted, an Invoke with a linked ID and a global
    # operation code, and a ReturnResultNotLast.
    cont = message(
        "continue",
        bytes.fromhex("2002"),
        bytes.fromhex("00001001"),
        [invoke(-2, "2.999.2", None, linked=1), result(1, 58, IDENTITY, last=False)],
    )
    aare = AARE()
    aare["protocol-version"] = univ.BitString("'1'B").subtype(implicitTag=ctx(0))
    aare["application-context-name"] = CONTEXT
    aare["result"] = 0
    aare["result-source-diagnostic"]["dialogue-service-user"] = 0
    set_dialogue(cont["continue"], "dialoguePortion", DIALOGUE_AS, aare)
    yield "continue_aare_linked_invoke_result_not_last", cont

    # End: ReturnResultLast with a result, ReturnResultLast without, a
    # ReturnError with a parameter.
    yield "end_results_and_error", message(
        "end",
        None,
        bytes.fromhex("01"),
        [result(1, 58, IDENTITY), result(2), error(3, 1, bytes.fromhex("3000"))],
    )

    # End: one Reject of each problem class, the first with the NULL invoke ID.
    yield "end_rejects", message(
        "end",
        None,
        bytes.fromhex("010203"),
        [
            reject(None, "generalProblem", 1),
            reject(5, "invokeProblem", 2),
            reject(-127, "returnResultProblem", 1),
            reject(127, "returnErrorProblem", 4),
        ],
    )

    # Abort with a P-Abort cause.
    p_abort = message("abort", None, bytes.fromhex("00001001"))
    p_abort["abort"]["reason"]["p-abortCause"] = 3
    yield "abort_p_abort_cause", p_abort

    # Abort with an ABRT APDU carrying user information.
    u_abort = message("abort", None, bytes.fromhex("00001001"))
    abrt = ABRT()
    abrt["abort-source"] = 0
    user_information(abrt)
    set_dialogue(u_abort["abort"]["reason"], "u-abortCause", DIALOGUE_AS, abrt)
    yield "abort_u_abort_abrt", u_abort

    # Abort with a refusing AARE.
    refused = message("abort", None, bytes.fromhex("00001001"))
    aare = AARE()
    aare["protocol-version"] = univ.BitString("'1'B").subtype(implicitTag=ctx(0))
    aare["application-context-name"] = CONTEXT
    aare["result"] = 1
    aare["result-source-diagnostic"]["dialogue-service-provider"] = 2
    set_dialogue(refused["abort"]["reason"], "u-abortCause", DIALOGUE_AS, aare)
    yield "abort_u_abort_aare_rejected", refused

    # Abort with nothing but the transaction ID.
    yield "abort_bare", message("abort", None, bytes.fromhex("ff"))

    # Unidirectional: AUDT, one Invoke.
    uni = message("unidirectional", None, None, [invoke(0, 58, NUMBER)])
    audt = AARQ()
    audt["protocol-version"] = univ.BitString("'1'B").subtype(implicitTag=ctx(0))
    audt["application-context-name"] = CONTEXT
    set_dialogue(uni["unidirectional"], "dialoguePortion", UNIDIALOGUE_AS, audt)
    yield "unidirectional_audt_invoke", uni

    # Begin without the DEFAULT protocol version in the AARQ.
    plain = message("begin", bytes.fromhex("0a0b0c"), None, None)
    aarq = AARQ()
    aarq["application-context-name"] = CONTEXT
    set_dialogue(plain["begin"], "dialoguePortion", DIALOGUE_AS, aarq)
    yield "begin_aarq_default_version", plain


def main():
    for name, value in vectors():
        print(f"{name} definite   {encoder.encode(value).hex()}")
        print(f"{name} indefinite {encoder.encode(value, defMode=False).hex()}")


if __name__ == "__main__":
    main()
