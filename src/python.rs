//! PyO3 bindings — `pip install ss7-tcap` gives a Rust-backed wheel exposing the
//! **same** TCAP (ITU-T Q.771–Q.775) BER codec the crate ships.
//!
//! Compiled only with `--features python`; the default crate build is pyo3-free, so
//! `cargo add tcap` / crates.io consumers pull zero pyo3. Two entry points share one
//! `add_contents()`:
//! * `#[pymodule] fn _tcap` — the standalone wheel (maturin `module-name`).
//! * `pub fn register(py, parent)` — mount `tcap` as a submodule of another
//!   extension, so a host can expose tcap without a second shared object.
//!
//! The Python surface is a faithful mirror of the Rust one. TCAP messages carry
//! opaque, application-decoded content (operation arguments, the dialogue
//! `EXTERNAL`) as raw BER — so those fields are `bytes` on the Python side,
//! exactly as the Rust codec keeps them as `rasn::types::Any`. Transaction ids
//! (OTID/DTID) are `bytes`; invoke ids, local operation/error codes, the P-Abort
//! cause and the Reject problem class and code are `int`. Each message class
//! builds and `.encode() -> bytes`; the module-level `decode(bytes)` dispatches
//! on the transaction tag and returns the matching class, or raises `TcapError`
//! with the `DecodeProblem` attached as `.problem` when any part of the message
//! was not understood. `decode_detailed(bytes)` returns the message or the
//! `DecodeProblem` without raising.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};

use rasn::types::{Any, ObjectIdentifier, OctetString};

use crate::dialogue::{
    AbortSource, AssociateSourceDiagnostic, DialogueContent, DialoguePdu as CoreDialoguePdu,
    External, ProtocolVersion,
};
use crate::{
    Abort, AbortReason, Begin, Component, Continue, DecodeProblem, Decoded, DialoguePortion, End,
    ErrorCode, Fault, GeneralProblem, Invoke, InvokeId, InvokeProblem, OperationCode, PAbortCause,
    Problem, Reject, ReturnError, ReturnErrorProblem, ReturnResult, ReturnResultProblem,
    ReturnResultValue, Sublayer, TcapError as CoreTcapError, TcapMessage, Unidirectional,
};

// ── Error mapping ───────────────────────────────────────────────────────────
create_exception!(
    tcap,
    TcapError,
    PyException,
    "TCAP protocol / codec error (ITU-T Q.771–Q.775)."
);

fn tcap_err(e: CoreTcapError) -> PyErr {
    TcapError::new_err(e.to_string())
}

/// The error for a message that was not fully understood: a `TcapError` whose
/// `problem` attribute is the [`PyDecodeProblem`].
fn malformed_err(py: Python<'_>, problem: DecodeProblem) -> PyErr {
    let err = TcapError::new_err(format!("malformed message: {problem}"));
    match Bound::new(py, PyDecodeProblem { inner: problem }) {
        Ok(attached) => match err.value(py).setattr("problem", attached) {
            Ok(()) => err,
            Err(failure) => failure,
        },
        Err(failure) => failure,
    }
}

// ── Q.773 transaction tags (APPLICATION class) ──────────────────────────────
/// Transaction PDU tag: Unidirectional `[APPLICATION 1]` (0x61 constructed).
pub const TAG_UNIDIRECTIONAL: u8 = 0x61;
/// Transaction PDU tag: Begin `[APPLICATION 2]` (0x62 constructed).
pub const TAG_BEGIN: u8 = 0x62;
/// Transaction PDU tag: End `[APPLICATION 4]` (0x64 constructed).
pub const TAG_END: u8 = 0x64;
/// Transaction PDU tag: Continue `[APPLICATION 5]` (0x65 constructed).
pub const TAG_CONTINUE: u8 = 0x65;
/// Transaction PDU tag: Abort `[APPLICATION 7]` (0x67 constructed).
pub const TAG_ABORT: u8 = 0x67;

// ── Component types (Q.773 §3.2, CONTEXT class) ─────────────────────────────
/// Component type: Invoke `[CONTEXT 1]`.
pub const COMPONENT_INVOKE: u8 = 1;
/// Component type: ReturnResult (Last) `[CONTEXT 2]`.
pub const COMPONENT_RETURN_RESULT_LAST: u8 = 2;
/// Component type: ReturnError `[CONTEXT 3]`.
pub const COMPONENT_RETURN_ERROR: u8 = 3;
/// Component type: Reject `[CONTEXT 4]`.
pub const COMPONENT_REJECT: u8 = 4;
/// Component type: ReturnResult (Not Last) `[CONTEXT 7]`.
pub const COMPONENT_RETURN_RESULT_NOT_LAST: u8 = 7;

// ── OperationCode helper ────────────────────────────────────────────────────
/// A TCAP operation code — either a `local` integer or a `global` OID.
///
/// Construct with `OperationCode.local(int)` or `OperationCode.global_(oid_arcs)`.
#[pyclass(name = "OperationCode", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyOperationCode {
    inner: OperationCode,
}

#[pymethods]
impl PyOperationCode {
    /// A local (integer) operation code.
    #[staticmethod]
    fn local(value: i64) -> Self {
        Self {
            inner: OperationCode::Local(value),
        }
    }

    /// A global (OID) operation code, from a sequence of arcs (e.g. `[0, 4, 0, 0, 1, 0, 21, 3]`).
    #[staticmethod]
    #[pyo3(name = "global_")]
    fn global_(arcs: Vec<u32>) -> PyResult<Self> {
        let oid = ObjectIdentifier::new(arcs)
            .ok_or_else(|| TcapError::new_err("invalid object identifier arcs"))?;
        Ok(Self {
            inner: OperationCode::Global(oid),
        })
    }

    /// `True` if this is a local (integer) code.
    #[getter]
    fn is_local(&self) -> bool {
        matches!(self.inner, OperationCode::Local(_))
    }

    /// The local integer value, or `None` for a global code.
    #[getter]
    fn value(&self) -> Option<i64> {
        match &self.inner {
            OperationCode::Local(v) => Some(*v),
            OperationCode::Global(_) => None,
        }
    }

    /// The OID arcs, or `None` for a local code.
    #[getter]
    fn oid(&self) -> Option<Vec<u32>> {
        match &self.inner {
            OperationCode::Global(oid) => Some(oid.to_vec()),
            OperationCode::Local(_) => None,
        }
    }

    fn __repr__(&self) -> String {
        format!("OperationCode({})", self.inner)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

// ── ErrorCode helper ────────────────────────────────────────────────────────
/// A TCAP error code — either a `local` integer or a `global` OID.
#[pyclass(name = "ErrorCode", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyErrorCode {
    inner: ErrorCode,
}

#[pymethods]
impl PyErrorCode {
    /// A local (integer) error code.
    #[staticmethod]
    fn local(value: i64) -> Self {
        Self {
            inner: ErrorCode::Local(value),
        }
    }

    /// A global (OID) error code, from a sequence of arcs.
    #[staticmethod]
    #[pyo3(name = "global_")]
    fn global_(arcs: Vec<u32>) -> PyResult<Self> {
        let oid = ObjectIdentifier::new(arcs)
            .ok_or_else(|| TcapError::new_err("invalid object identifier arcs"))?;
        Ok(Self {
            inner: ErrorCode::Global(oid),
        })
    }

    /// `True` if this is a local (integer) code.
    #[getter]
    fn is_local(&self) -> bool {
        matches!(self.inner, ErrorCode::Local(_))
    }

    /// The local integer value, or `None` for a global code.
    #[getter]
    fn value(&self) -> Option<i64> {
        match &self.inner {
            ErrorCode::Local(v) => Some(*v),
            ErrorCode::Global(_) => None,
        }
    }

    /// The OID arcs, or `None` for a local code.
    #[getter]
    fn oid(&self) -> Option<Vec<u32>> {
        match &self.inner {
            ErrorCode::Global(oid) => Some(oid.to_vec()),
            ErrorCode::Local(_) => None,
        }
    }

    fn __repr__(&self) -> String {
        format!("ErrorCode({})", self.inner)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

// ── Components ──────────────────────────────────────────────────────────────
/// A TCAP Invoke component (Q.773 §3.2) — carries an operation for the peer.
#[pyclass(name = "Invoke", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyInvoke {
    #[pyo3(get, set)]
    pub invoke_id: InvokeId,
    #[pyo3(get, set)]
    pub linked_id: Option<InvokeId>,
    #[pyo3(get, set)]
    pub operation_code: PyOperationCode,
    /// Opaque operation argument (BER), decoded by the application (e.g. MAP).
    parameter: Option<Vec<u8>>,
}

#[pymethods]
impl PyInvoke {
    #[new]
    #[pyo3(signature = (invoke_id, operation_code, *, linked_id = None, parameter = None))]
    fn new(
        invoke_id: InvokeId,
        operation_code: PyOperationCode,
        linked_id: Option<InvokeId>,
        parameter: Option<Vec<u8>>,
    ) -> Self {
        Self {
            invoke_id,
            linked_id,
            operation_code,
            parameter,
        }
    }

    /// The opaque operation argument as `bytes` (or `None`).
    #[getter]
    fn parameter<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.parameter.as_ref().map(|p| PyBytes::new(py, p))
    }

    #[setter]
    fn set_parameter(&mut self, parameter: Option<Vec<u8>>) {
        self.parameter = parameter;
    }

    fn __repr__(&self) -> String {
        format!(
            "Invoke(invoke_id={}, operation_code={})",
            self.invoke_id, self.operation_code.inner
        )
    }
}

impl PyInvoke {
    fn to_core(&self) -> Invoke {
        Invoke {
            invoke_id: self.invoke_id,
            linked_id: self.linked_id,
            operation_code: self.operation_code.inner.clone(),
            parameter: self.parameter.clone().map(Any::new),
        }
    }

    fn from_core(inv: Invoke) -> Self {
        Self {
            invoke_id: inv.invoke_id,
            linked_id: inv.linked_id,
            operation_code: PyOperationCode {
                inner: inv.operation_code,
            },
            parameter: inv.parameter.map(|a| a.as_bytes().to_vec()),
        }
    }
}

/// A TCAP ReturnResult component — the (successful) result of an Invoke.
///
/// `last=True` (the default) uses `[CONTEXT 2]` (ReturnResultLast); `last=False`
/// uses `[CONTEXT 7]` (ReturnResultNotLast), for segmented results.
#[pyclass(name = "ReturnResult", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyReturnResult {
    #[pyo3(get, set)]
    pub invoke_id: InvokeId,
    #[pyo3(get, set)]
    pub last: bool,
    #[pyo3(get, set)]
    pub operation_code: Option<PyOperationCode>,
    parameter: Option<Vec<u8>>,
}

#[pymethods]
impl PyReturnResult {
    #[new]
    #[pyo3(signature = (invoke_id, *, operation_code = None, parameter = None, last = true))]
    fn new(
        invoke_id: InvokeId,
        operation_code: Option<PyOperationCode>,
        parameter: Option<Vec<u8>>,
        last: bool,
    ) -> Self {
        Self {
            invoke_id,
            last,
            operation_code,
            parameter,
        }
    }

    /// The opaque result parameter as `bytes` (or `None`).
    #[getter]
    fn parameter<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.parameter.as_ref().map(|p| PyBytes::new(py, p))
    }

    #[setter]
    fn set_parameter(&mut self, parameter: Option<Vec<u8>>) {
        self.parameter = parameter;
    }

    fn __repr__(&self) -> String {
        format!(
            "ReturnResult(invoke_id={}, last={})",
            self.invoke_id, self.last
        )
    }
}

impl PyReturnResult {
    fn to_core(&self) -> ReturnResult {
        let result = self.operation_code.as_ref().map(|op| ReturnResultValue {
            operation_code: op.inner.clone(),
            parameter: self.parameter.clone().map(Any::new),
        });
        ReturnResult {
            invoke_id: self.invoke_id,
            result,
        }
    }

    fn from_core(rr: ReturnResult, last: bool) -> Self {
        let (operation_code, parameter) = match rr.result {
            Some(v) => (
                Some(PyOperationCode {
                    inner: v.operation_code,
                }),
                v.parameter.map(|a| a.as_bytes().to_vec()),
            ),
            None => (None, None),
        };
        Self {
            invoke_id: rr.invoke_id,
            last,
            operation_code,
            parameter,
        }
    }
}

/// A TCAP ReturnError component — a (failure) response to an Invoke.
#[pyclass(name = "ReturnError", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyReturnError {
    #[pyo3(get, set)]
    pub invoke_id: InvokeId,
    #[pyo3(get, set)]
    pub error_code: PyErrorCode,
    parameter: Option<Vec<u8>>,
}

#[pymethods]
impl PyReturnError {
    #[new]
    #[pyo3(signature = (invoke_id, error_code, *, parameter = None))]
    fn new(invoke_id: InvokeId, error_code: PyErrorCode, parameter: Option<Vec<u8>>) -> Self {
        Self {
            invoke_id,
            error_code,
            parameter,
        }
    }

    /// The opaque error parameter as `bytes` (or `None`).
    #[getter]
    fn parameter<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.parameter.as_ref().map(|p| PyBytes::new(py, p))
    }

    #[setter]
    fn set_parameter(&mut self, parameter: Option<Vec<u8>>) {
        self.parameter = parameter;
    }

    fn __repr__(&self) -> String {
        format!(
            "ReturnError(invoke_id={}, error_code={})",
            self.invoke_id, self.error_code.inner
        )
    }
}

impl PyReturnError {
    fn to_core(&self) -> ReturnError {
        ReturnError {
            invoke_id: self.invoke_id,
            error_code: self.error_code.inner.clone(),
            parameter: self.parameter.clone().map(Any::new),
        }
    }

    fn from_core(re: ReturnError) -> Self {
        Self {
            invoke_id: re.invoke_id,
            error_code: PyErrorCode {
                inner: re.error_code,
            },
            parameter: re.parameter.map(|a| a.as_bytes().to_vec()),
        }
    }
}

// ── Reject problem classes and codes (Q.773 Tables 25 to 29) ────────────────
/// Problem class: general problem `[0]`.
pub const PROBLEM_GENERAL: u8 = 0;
/// Problem class: invoke problem `[1]`.
pub const PROBLEM_INVOKE: u8 = 1;
/// Problem class: return result problem `[2]`.
pub const PROBLEM_RETURN_RESULT: u8 = 2;
/// Problem class: return error problem `[3]`.
pub const PROBLEM_RETURN_ERROR: u8 = 3;

fn problem_parts(problem: Problem) -> (u8, i64) {
    match problem {
        Problem::General(code) => (PROBLEM_GENERAL, code.value()),
        Problem::Invoke(code) => (PROBLEM_INVOKE, code.value()),
        Problem::ReturnResult(code) => (PROBLEM_RETURN_RESULT, code.value()),
        Problem::ReturnError(code) => (PROBLEM_RETURN_ERROR, code.value()),
    }
}

fn problem_from_parts(problem_type: u8, problem_code: i64) -> PyResult<Problem> {
    match problem_type {
        PROBLEM_GENERAL => Ok(Problem::General(GeneralProblem(problem_code))),
        PROBLEM_INVOKE => Ok(Problem::Invoke(InvokeProblem(problem_code))),
        PROBLEM_RETURN_RESULT => Ok(Problem::ReturnResult(ReturnResultProblem(problem_code))),
        PROBLEM_RETURN_ERROR => Ok(Problem::ReturnError(ReturnErrorProblem(problem_code))),
        _ => Err(TcapError::new_err(
            "problem_type must be 0 (general), 1 (invoke), 2 (return result) or 3 (return error)",
        )),
    }
}

/// A TCAP Reject component — rejects a received component.
///
/// `invoke_id` is `None` when the invoke ID of the rejected component could not
/// be derived (sent as a NULL). `problem_type` is the problem class
/// (`PROBLEM_GENERAL` / `PROBLEM_INVOKE` / `PROBLEM_RETURN_RESULT` /
/// `PROBLEM_RETURN_ERROR`) and `problem_code` its value (Q.773 Tables 26 to 29).
#[pyclass(name = "Reject", module = "tcap._tcap", from_py_object)]
#[derive(Clone)]
pub struct PyReject {
    #[pyo3(get, set)]
    pub invoke_id: Option<InvokeId>,
    #[pyo3(get)]
    pub problem_type: u8,
    #[pyo3(get, set)]
    pub problem_code: i64,
}

#[pymethods]
impl PyReject {
    #[new]
    fn new(invoke_id: Option<InvokeId>, problem_type: u8, problem_code: i64) -> PyResult<Self> {
        problem_from_parts(problem_type, problem_code)?;
        Ok(Self {
            invoke_id,
            problem_type,
            problem_code,
        })
    }

    #[setter]
    fn set_problem_type(&mut self, problem_type: u8) -> PyResult<()> {
        problem_from_parts(problem_type, self.problem_code)?;
        self.problem_type = problem_type;
        Ok(())
    }

    fn __repr__(&self) -> String {
        match self.to_core() {
            Ok(reject) => format!("Reject({})", Component::Reject(reject)),
            Err(_) => "Reject(invalid)".to_string(),
        }
    }
}

impl PyReject {
    fn to_core(&self) -> PyResult<Reject> {
        Ok(Reject {
            invoke_id: self.invoke_id,
            problem: problem_from_parts(self.problem_type, self.problem_code)?,
        })
    }

    fn from_core(rj: Reject) -> Self {
        let (problem_type, problem_code) = problem_parts(rj.problem);
        Self {
            invoke_id: rj.invoke_id,
            problem_type,
            problem_code,
        }
    }
}

// ── Component conversion (Python object → core Component) ────────────────────
/// Convert a Python component object (Invoke / ReturnResult / ReturnError /
/// Reject) into a core [`Component`], preserving Last vs NotLast for ReturnResult.
fn py_component_to_core(py: Python<'_>, obj: &Py<PyAny>) -> PyResult<Component> {
    let bound = obj.bind(py);
    if let Ok(inv) = bound.extract::<PyInvoke>() {
        Ok(Component::Invoke(inv.to_core()))
    } else if let Ok(rr) = bound.extract::<PyReturnResult>() {
        let core = rr.to_core();
        Ok(if rr.last {
            Component::ReturnResultLast(core)
        } else {
            Component::ReturnResultNotLast(core)
        })
    } else if let Ok(re) = bound.extract::<PyReturnError>() {
        Ok(Component::ReturnError(re.to_core()))
    } else if let Ok(rj) = bound.extract::<PyReject>() {
        Ok(Component::Reject(rj.to_core()?))
    } else {
        Err(TcapError::new_err(
            "component must be an Invoke, ReturnResult, ReturnError, or Reject",
        ))
    }
}

/// Convert a core [`Component`] into the matching Python component object.
fn core_component_to_py(py: Python<'_>, comp: Component) -> PyResult<Py<PyAny>> {
    let any = match comp {
        Component::Invoke(inv) => Bound::new(py, PyInvoke::from_core(inv))?.into_any(),
        Component::ReturnResultLast(rr) => {
            Bound::new(py, PyReturnResult::from_core(rr, true))?.into_any()
        }
        Component::ReturnResultNotLast(rr) => {
            Bound::new(py, PyReturnResult::from_core(rr, false))?.into_any()
        }
        Component::ReturnError(re) => Bound::new(py, PyReturnError::from_core(re))?.into_any(),
        Component::Reject(rj) => Bound::new(py, PyReject::from_core(rj))?.into_any(),
    };
    Ok(any.unbind())
}

fn components_to_core(py: Python<'_>, comps: &[Py<PyAny>]) -> PyResult<Vec<Component>> {
    comps.iter().map(|c| py_component_to_core(py, c)).collect()
}

fn components_to_py(py: Python<'_>, comps: Vec<Component>) -> PyResult<Vec<Py<PyAny>>> {
    comps
        .into_iter()
        .map(|c| core_component_to_py(py, c))
        .collect()
}

// ── Transaction messages ────────────────────────────────────────────────────
/// A TCAP **Begin** transaction (`[APPLICATION 2]`) — opens a dialogue.
#[pyclass(name = "Begin", module = "tcap._tcap", skip_from_py_object)]
pub struct PyBegin {
    otid: Vec<u8>,
    dialogue_portion: Option<Vec<u8>>,
    components: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyBegin {
    #[new]
    #[pyo3(signature = (otid, *, components = None, dialogue_portion = None))]
    fn new(
        otid: Vec<u8>,
        components: Option<Vec<Py<PyAny>>>,
        dialogue_portion: Option<Vec<u8>>,
    ) -> Self {
        Self {
            otid,
            dialogue_portion,
            components: components.unwrap_or_default(),
        }
    }

    /// Originating Transaction ID (`bytes`, 1–4 octets).
    #[getter]
    fn otid<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.otid)
    }

    /// The dialogue portion (opaque EXTERNAL BER) as `bytes`, or `None`.
    #[getter]
    fn dialogue_portion<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.dialogue_portion.as_ref().map(|d| PyBytes::new(py, d))
    }

    /// The component list (Invoke / ReturnResult / … objects).
    #[getter]
    fn components(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.components.iter().map(|c| c.clone_ref(py)).collect()
    }

    /// Encode this Begin to Q.773-compliant BER `bytes`.
    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let components = components_to_core(py, &self.components)?;
        let begin = Begin {
            otid: OctetString::from(self.otid.clone()),
            dialogue_portion: self.dialogue_portion.clone().map(|d| DialoguePortion {
                external: Any::new(d),
            }),
            components: if components.is_empty() {
                None
            } else {
                Some(components)
            },
        };
        let bytes = crate::encode(&TcapMessage::Begin(begin)).map_err(tcap_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn __repr__(&self) -> String {
        format!(
            "Begin(otid={}, {} components)",
            hex::encode(&self.otid),
            self.components.len()
        )
    }
}

/// A TCAP **Continue** transaction (`[APPLICATION 5]`) — mid-dialogue exchange.
#[pyclass(name = "Continue", module = "tcap._tcap", skip_from_py_object)]
pub struct PyContinue {
    otid: Vec<u8>,
    dtid: Vec<u8>,
    dialogue_portion: Option<Vec<u8>>,
    components: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyContinue {
    #[new]
    #[pyo3(signature = (otid, dtid, *, components = None, dialogue_portion = None))]
    fn new(
        otid: Vec<u8>,
        dtid: Vec<u8>,
        components: Option<Vec<Py<PyAny>>>,
        dialogue_portion: Option<Vec<u8>>,
    ) -> Self {
        Self {
            otid,
            dtid,
            dialogue_portion,
            components: components.unwrap_or_default(),
        }
    }

    /// Originating Transaction ID (`bytes`).
    #[getter]
    fn otid<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.otid)
    }

    /// Destination Transaction ID (`bytes`).
    #[getter]
    fn dtid<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.dtid)
    }

    /// The dialogue portion (opaque EXTERNAL BER) as `bytes`, or `None`.
    #[getter]
    fn dialogue_portion<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.dialogue_portion.as_ref().map(|d| PyBytes::new(py, d))
    }

    /// The component list.
    #[getter]
    fn components(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.components.iter().map(|c| c.clone_ref(py)).collect()
    }

    /// Encode this Continue to Q.773-compliant BER `bytes`.
    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let components = components_to_core(py, &self.components)?;
        let cont = Continue {
            otid: OctetString::from(self.otid.clone()),
            dtid: OctetString::from(self.dtid.clone()),
            dialogue_portion: self.dialogue_portion.clone().map(|d| DialoguePortion {
                external: Any::new(d),
            }),
            components: if components.is_empty() {
                None
            } else {
                Some(components)
            },
        };
        let bytes = crate::encode(&TcapMessage::Continue(cont)).map_err(tcap_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn __repr__(&self) -> String {
        format!(
            "Continue(otid={}, dtid={}, {} components)",
            hex::encode(&self.otid),
            hex::encode(&self.dtid),
            self.components.len()
        )
    }
}

/// A TCAP **End** transaction (`[APPLICATION 4]`) — closes a dialogue.
#[pyclass(name = "End", module = "tcap._tcap", skip_from_py_object)]
pub struct PyEnd {
    dtid: Vec<u8>,
    dialogue_portion: Option<Vec<u8>>,
    components: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyEnd {
    #[new]
    #[pyo3(signature = (dtid, *, components = None, dialogue_portion = None))]
    fn new(
        dtid: Vec<u8>,
        components: Option<Vec<Py<PyAny>>>,
        dialogue_portion: Option<Vec<u8>>,
    ) -> Self {
        Self {
            dtid,
            dialogue_portion,
            components: components.unwrap_or_default(),
        }
    }

    /// Destination Transaction ID (`bytes`).
    #[getter]
    fn dtid<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.dtid)
    }

    /// The dialogue portion (opaque EXTERNAL BER) as `bytes`, or `None`.
    #[getter]
    fn dialogue_portion<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.dialogue_portion.as_ref().map(|d| PyBytes::new(py, d))
    }

    /// The component list.
    #[getter]
    fn components(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.components.iter().map(|c| c.clone_ref(py)).collect()
    }

    /// Encode this End to Q.773-compliant BER `bytes`.
    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let components = components_to_core(py, &self.components)?;
        let end = End {
            dtid: OctetString::from(self.dtid.clone()),
            dialogue_portion: self.dialogue_portion.clone().map(|d| DialoguePortion {
                external: Any::new(d),
            }),
            components: if components.is_empty() {
                None
            } else {
                Some(components)
            },
        };
        let bytes = crate::encode(&TcapMessage::End(end)).map_err(tcap_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn __repr__(&self) -> String {
        format!(
            "End(dtid={}, {} components)",
            hex::encode(&self.dtid),
            self.components.len()
        )
    }
}

// ── P-Abort causes (Q.773 Table 12) ─────────────────────────────────────────
/// P-Abort cause: unrecognizedMessageType (0).
pub const P_ABORT_UNRECOGNIZED_MESSAGE_TYPE: i64 = 0;
/// P-Abort cause: unrecognizedTransactionID (1).
pub const P_ABORT_UNRECOGNIZED_TRANSACTION_ID: i64 = 1;
/// P-Abort cause: badlyFormattedTransactionPortion (2).
pub const P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION: i64 = 2;
/// P-Abort cause: incorrectTransactionPortion (3).
pub const P_ABORT_INCORRECT_TRANSACTION_PORTION: i64 = 3;
/// P-Abort cause: resourceLimitation (4).
pub const P_ABORT_RESOURCE_LIMITATION: i64 = 4;

/// A TCAP **Abort** transaction (`[APPLICATION 7]`) — aborts a dialogue.
///
/// The reason is a choice: `p_abort_cause` (an `int`, see `P_ABORT_*`) when
/// the transaction sub-layer aborts, or `dialogue_portion` (the `EXTERNAL`
/// `bytes`: an ABRT or AARE APDU, or user information) when the user or the
/// component sub-layer aborts. At most one of the two; neither is an abort by
/// the user without information.
#[pyclass(name = "Abort", module = "tcap._tcap", skip_from_py_object)]
pub struct PyAbort {
    dtid: Vec<u8>,
    p_abort_cause: Option<i64>,
    dialogue_portion: Option<Vec<u8>>,
}

#[pymethods]
impl PyAbort {
    #[new]
    #[pyo3(signature = (dtid, *, p_abort_cause = None, dialogue_portion = None))]
    fn new(
        dtid: Vec<u8>,
        p_abort_cause: Option<i64>,
        dialogue_portion: Option<Vec<u8>>,
    ) -> PyResult<Self> {
        if p_abort_cause.is_some() && dialogue_portion.is_some() {
            return Err(TcapError::new_err(
                "an Abort carries a P-Abort cause or a dialogue portion, not both",
            ));
        }
        Ok(Self {
            dtid,
            p_abort_cause,
            dialogue_portion,
        })
    }

    /// Destination Transaction ID (`bytes`).
    #[getter]
    fn dtid<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.dtid)
    }

    /// The P-Abort cause (`int`), or `None`.
    #[getter]
    fn p_abort_cause(&self) -> Option<i64> {
        self.p_abort_cause
    }

    /// The user abort information: the dialogue-portion `EXTERNAL` as `bytes`,
    /// or `None`.
    #[getter]
    fn dialogue_portion<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.dialogue_portion.as_ref().map(|d| PyBytes::new(py, d))
    }

    /// Encode this Abort to Q.773-compliant BER `bytes`.
    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = crate::encode(&TcapMessage::Abort(self.to_core())).map_err(tcap_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn __repr__(&self) -> String {
        format!("{}", TcapMessage::Abort(self.to_core()))
    }
}

impl PyAbort {
    fn to_core(&self) -> Abort {
        let reason = match (&self.p_abort_cause, &self.dialogue_portion) {
            (Some(cause), _) => Some(AbortReason::PAbort(PAbortCause(*cause))),
            (None, Some(portion)) => Some(AbortReason::UAbort(DialoguePortion {
                external: Any::new(portion.clone()),
            })),
            (None, None) => None,
        };
        Abort {
            dtid: OctetString::from(self.dtid.clone()),
            reason,
        }
    }

    fn from_core(abort: Abort) -> Self {
        let (p_abort_cause, dialogue_portion) = match abort.reason {
            Some(AbortReason::PAbort(cause)) => (Some(cause.value()), None),
            Some(AbortReason::UAbort(portion)) => {
                (None, Some(portion.external.as_bytes().to_vec()))
            }
            None => (None, None),
        };
        Self {
            dtid: abort.dtid.to_vec(),
            p_abort_cause,
            dialogue_portion,
        }
    }
}

/// A TCAP **Unidirectional** transaction (`[APPLICATION 1]`) — fire-and-forget.
#[pyclass(name = "Unidirectional", module = "tcap._tcap", skip_from_py_object)]
pub struct PyUnidirectional {
    dialogue_portion: Option<Vec<u8>>,
    components: Vec<Py<PyAny>>,
}

#[pymethods]
impl PyUnidirectional {
    #[new]
    #[pyo3(signature = (*, components = None, dialogue_portion = None))]
    fn new(components: Option<Vec<Py<PyAny>>>, dialogue_portion: Option<Vec<u8>>) -> Self {
        Self {
            dialogue_portion,
            components: components.unwrap_or_default(),
        }
    }

    /// The dialogue portion (opaque EXTERNAL BER) as `bytes`, or `None`.
    #[getter]
    fn dialogue_portion<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.dialogue_portion.as_ref().map(|d| PyBytes::new(py, d))
    }

    /// The component list.
    #[getter]
    fn components(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.components.iter().map(|c| c.clone_ref(py)).collect()
    }

    /// Encode this Unidirectional to Q.773-compliant BER `bytes`.
    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let components = components_to_core(py, &self.components)?;
        let uni = Unidirectional {
            dialogue_portion: self.dialogue_portion.clone().map(|d| DialoguePortion {
                external: Any::new(d),
            }),
            components,
        };
        let bytes = crate::encode(&TcapMessage::Unidirectional(uni)).map_err(tcap_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn __repr__(&self) -> String {
        format!("Unidirectional({} components)", self.components.len())
    }
}

// ── Dialogue portion (AARQ / AARE / ABRT) ───────────────────────────────────
/// ABRT-source: dialogue-service-user(0).
pub const ABORT_SOURCE_USER: i64 = 0;
/// ABRT-source: dialogue-service-provider(1).
pub const ABORT_SOURCE_PROVIDER: i64 = 1;

fn oid_from_arcs(arcs: Vec<u32>) -> PyResult<ObjectIdentifier> {
    ObjectIdentifier::new(arcs).ok_or_else(|| TcapError::new_err("invalid object identifier arcs"))
}

/// A decoded TCAP dialogue PDU (AARQ / AARE / ABRT / AUDT), as read back from a
/// message's `dialogue_portion` by [`parse_dialogue_portion`].
///
/// Inspect `.pdu_type` (`"AARQ"` / `"AARE"` / `"ABRT"` / `"AUDT"`); AARQ, AARE
/// and AUDT expose `.application_context` (OID arcs) and `.version1`; AARE also
/// `.result` (0 = accepted, 1 = reject-permanent) and
/// `.result_source_diagnostic` (a `(source, value)` pair, source 1 = user,
/// 2 = provider); ABRT exposes `.abort_source` (0 = user, 1 = provider).
#[pyclass(name = "DialoguePdu", module = "tcap._tcap", skip_from_py_object)]
pub struct PyDialoguePdu {
    inner: CoreDialoguePdu,
}

#[pymethods]
impl PyDialoguePdu {
    /// `"AARQ"`, `"AARE"`, `"ABRT"`, or `"AUDT"`.
    #[getter]
    fn pdu_type(&self) -> &'static str {
        match &self.inner {
            CoreDialoguePdu::Aarq { .. } => "AARQ",
            CoreDialoguePdu::Aare { .. } => "AARE",
            CoreDialoguePdu::Abrt { .. } => "ABRT",
            CoreDialoguePdu::Audt { .. } => "AUDT",
        }
    }

    /// The application-context-name OID arcs (AARQ / AARE / AUDT), else `None`.
    #[getter]
    fn application_context(&self) -> Option<Vec<u32>> {
        match &self.inner {
            CoreDialoguePdu::Aarq {
                application_context_name,
                ..
            }
            | CoreDialoguePdu::Aare {
                application_context_name,
                ..
            }
            | CoreDialoguePdu::Audt {
                application_context_name,
                ..
            } => Some(application_context_name.to_vec()),
            CoreDialoguePdu::Abrt { .. } => None,
        }
    }

    /// Whether the protocol version lists version 1 (AARQ / AARE / AUDT), else
    /// `None`. An AARQ with `False` is answered with
    /// `dialogue_aare_reject(context, 2, 2)` in an Abort (Q.774 3.2.3).
    #[getter]
    fn version1(&self) -> Option<bool> {
        match &self.inner {
            CoreDialoguePdu::Aarq {
                protocol_version, ..
            }
            | CoreDialoguePdu::Aare {
                protocol_version, ..
            }
            | CoreDialoguePdu::Audt {
                protocol_version, ..
            } => Some(*protocol_version == ProtocolVersion::Version1),
            CoreDialoguePdu::Abrt { .. } => None,
        }
    }

    /// The associate result INTEGER (AARE only): 0 accepted, 1 reject-permanent.
    /// `None` for the other PDUs.
    #[getter]
    fn result(&self) -> Option<i64> {
        match &self.inner {
            CoreDialoguePdu::Aare { result, .. } => Some(result.value()),
            _ => None,
        }
    }

    /// The result-source-diagnostic (AARE only) as `(source, value)` where
    /// source is 1 = dialogue-service-user, 2 = dialogue-service-provider.
    #[getter]
    fn result_source_diagnostic(&self) -> Option<(i64, i64)> {
        match &self.inner {
            CoreDialoguePdu::Aare {
                result_source_diagnostic,
                ..
            } => Some(match result_source_diagnostic {
                AssociateSourceDiagnostic::DialogueServiceUser(v) => (1, *v),
                AssociateSourceDiagnostic::DialogueServiceProvider(v) => (2, *v),
            }),
            _ => None,
        }
    }

    /// The abort source INTEGER (ABRT only): 0 = user, 1 = provider.
    #[getter]
    fn abort_source(&self) -> Option<i64> {
        match &self.inner {
            CoreDialoguePdu::Abrt { abort_source, .. } => Some(abort_source.value()),
            _ => None,
        }
    }

    /// The user-information `[30]` content octets, if present: the encodings
    /// of its `EXTERNAL` values, one after the other.
    #[getter]
    fn user_information<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        let ui = match &self.inner {
            CoreDialoguePdu::Aarq {
                user_information, ..
            }
            | CoreDialoguePdu::Aare {
                user_information, ..
            }
            | CoreDialoguePdu::Abrt {
                user_information, ..
            }
            | CoreDialoguePdu::Audt {
                user_information, ..
            } => user_information.as_ref(),
        };
        ui.map(|externals| {
            let content: Vec<u8> = externals.iter().flat_map(External::encode).collect();
            PyBytes::new(py, &content)
        })
    }

    fn __repr__(&self) -> String {
        format!("DialoguePdu({})", self.pdu_type())
    }
}

/// Build an **AARQ** dialogue portion carrying `application_context` (OID arcs),
/// returning the `EXTERNAL` `bytes` to pass as a message's `dialogue_portion`.
#[pyfunction]
fn dialogue_aarq<'py>(
    py: Python<'py>,
    application_context: Vec<u32>,
) -> PyResult<Bound<'py, PyBytes>> {
    let oid = oid_from_arcs(application_context)?;
    let dp = DialoguePortion::aarq(&oid);
    Ok(PyBytes::new(py, dp.external.as_bytes()))
}

/// Build an accepting **AARE** dialogue portion (result accepted(0),
/// diagnostic dialogue-service-user null(0)) carrying `application_context`
/// (OID arcs), returning the `EXTERNAL` `bytes`.
#[pyfunction]
fn dialogue_aare_accept<'py>(
    py: Python<'py>,
    application_context: Vec<u32>,
) -> PyResult<Bound<'py, PyBytes>> {
    let oid = oid_from_arcs(application_context)?;
    let dp = DialoguePortion::aare_accept(&oid);
    Ok(PyBytes::new(py, dp.external.as_bytes()))
}

/// Build a refusing **AARE** dialogue portion (result reject-permanent(1))
/// carrying `application_context` (OID arcs) and the diagnostic `(source,
/// value)`: source 1 = dialogue-service-user, 2 = dialogue-service-provider.
/// It travels as the `dialogue_portion` of an Abort. `(2, 2)` is the answer to
/// an AARQ that does not list protocol version 1 (Q.774 3.2.3).
#[pyfunction]
fn dialogue_aare_reject<'py>(
    py: Python<'py>,
    application_context: Vec<u32>,
    source: i64,
    value: i64,
) -> PyResult<Bound<'py, PyBytes>> {
    let oid = oid_from_arcs(application_context)?;
    let diagnostic = match source {
        1 => AssociateSourceDiagnostic::DialogueServiceUser(value),
        2 => AssociateSourceDiagnostic::DialogueServiceProvider(value),
        _ => {
            return Err(TcapError::new_err(
                "source must be 1 (dialogue-service-user) or 2 (dialogue-service-provider)",
            ))
        }
    };
    let dp = DialoguePortion::aare_reject(&oid, diagnostic);
    Ok(PyBytes::new(py, dp.external.as_bytes()))
}

/// Build an **AUDT** dialogue portion carrying `application_context` (OID
/// arcs), for a Unidirectional, returning the `EXTERNAL` `bytes`.
#[pyfunction]
fn dialogue_audt<'py>(
    py: Python<'py>,
    application_context: Vec<u32>,
) -> PyResult<Bound<'py, PyBytes>> {
    let oid = oid_from_arcs(application_context)?;
    let dp = DialoguePortion::audt(&oid);
    Ok(PyBytes::new(py, dp.external.as_bytes()))
}

/// Build an **ABRT** dialogue portion for `abort_source` (0 = user,
/// 1 = provider; see `ABORT_SOURCE_*`), returning the `EXTERNAL` `bytes`.
/// `dialogue_abrt(ABORT_SOURCE_PROVIDER)` is what the component sub-layer
/// sends for an incorrect dialogue portion (Q.774 3.2.2.1).
#[pyfunction]
fn dialogue_abrt<'py>(py: Python<'py>, abort_source: i64) -> PyResult<Bound<'py, PyBytes>> {
    let source = match abort_source {
        0 => AbortSource::DialogueServiceUser,
        1 => AbortSource::DialogueServiceProvider,
        _ => {
            return Err(TcapError::new_err(
                "abort_source must be 0 (user) or 1 (provider)",
            ))
        }
    };
    Ok(PyBytes::new(
        py,
        DialoguePortion::abrt(source).external.as_bytes(),
    ))
}

/// Parse a dialogue portion (the `EXTERNAL` `bytes` from a decoded message's
/// `dialogue_portion`).
///
/// Returns a `DialoguePdu` for an AARQ / AARE / ABRT / AUDT, and `None` for a
/// well-formed `EXTERNAL` that carries something else (user information in a
/// user-defined abstract syntax). Raises `TcapError` when the portion is
/// malformed.
#[pyfunction]
fn parse_dialogue_portion(py: Python<'_>, data: &[u8]) -> PyResult<Option<Py<PyAny>>> {
    let dp = DialoguePortion {
        external: Any::new(data.to_vec()),
    };
    match dp.parse() {
        Ok(DialogueContent::Pdu(pdu)) => Ok(Some(
            Bound::new(py, PyDialoguePdu { inner: pdu })?
                .into_any()
                .unbind(),
        )),
        Ok(DialogueContent::Unmodelled(_)) => Ok(None),
        Err(error) => Err(TcapError::new_err(error.to_string())),
    }
}

// ── What a damaged message yields ───────────────────────────────────────────
/// A message that was not fully understood, with what Q.774 needs to answer it.
///
/// * `sublayer`: `"transaction"` or `"component"`, the sub-layer that detected
///   the problem.
/// * `fault`: `"transaction_portion"`, `"dialogue_portion"` or `"component"`.
/// * `p_abort_cause`: the P-Abort cause for a transaction portion fault.
/// * `general_problem`, `component_index`, `component_type`, `invoke_id`: for a
///   component fault, the general problem code, the position of the component,
///   its type (`COMPONENT_*`, `None` if not recognized) and its invoke ID
///   (`None` if not derivable).
/// * `message_type`: the message tag (`TAG_*`), `None` if not recognized.
/// * `otid` / `dtid`: the transaction IDs that are derivable.
/// * `partial`: for a component fault, the message with the components before
///   the faulty one, which stand.
/// * `abort()` / `reject()`: the `Abort` message or the `Reject` component to
///   send, or `None` when Q.774 calls for none.
#[pyclass(name = "DecodeProblem", module = "tcap._tcap", skip_from_py_object)]
pub struct PyDecodeProblem {
    inner: DecodeProblem,
}

#[pymethods]
impl PyDecodeProblem {
    #[getter]
    fn sublayer(&self) -> &'static str {
        match self.inner.sublayer() {
            Sublayer::Transaction => "transaction",
            Sublayer::Component => "component",
        }
    }

    #[getter]
    fn fault(&self) -> &'static str {
        match self.inner.fault {
            Fault::TransactionPortion { .. } => "transaction_portion",
            Fault::DialoguePortion => "dialogue_portion",
            Fault::Component { .. } => "component",
        }
    }

    #[getter]
    fn p_abort_cause(&self) -> Option<i64> {
        self.inner.p_abort_cause().map(PAbortCause::value)
    }

    #[getter]
    fn general_problem(&self) -> Option<i64> {
        match self.inner.fault {
            Fault::Component { problem, .. } => Some(problem.value()),
            _ => None,
        }
    }

    #[getter]
    fn component_index(&self) -> Option<usize> {
        match self.inner.fault {
            Fault::Component { index, .. } => Some(index),
            _ => None,
        }
    }

    #[getter]
    fn component_type(&self) -> Option<u8> {
        match self.inner.fault {
            Fault::Component { component_type, .. } => component_type.map(|t| t.tag()),
            _ => None,
        }
    }

    #[getter]
    fn invoke_id(&self) -> Option<InvokeId> {
        match self.inner.fault {
            Fault::Component { invoke_id, .. } => invoke_id,
            _ => None,
        }
    }

    /// The message tag octet (`TAG_BEGIN` and so on), or `None`.
    #[getter]
    fn message_type(&self) -> Option<u8> {
        self.inner.message_type.map(|t| 0x60 | t.tag())
    }

    #[getter]
    fn otid<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.otid.as_ref().map(|id| PyBytes::new(py, id))
    }

    #[getter]
    fn dtid<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.dtid.as_ref().map(|id| PyBytes::new(py, id))
    }

    #[getter]
    fn partial(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .partial
            .clone()
            .map(|message| message_to_py(py, message))
            .transpose()
    }

    #[getter]
    fn detail(&self) -> &str {
        &self.inner.detail
    }

    /// The `Abort` to send to the originator, or `None`.
    fn abort(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .abort()
            .map(|abort| {
                Ok(Bound::new(py, PyAbort::from_core(abort))?
                    .into_any()
                    .unbind())
            })
            .transpose()
    }

    /// The `Reject` component to send, or `None`.
    fn reject(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .reject()
            .map(|reject| {
                Ok(Bound::new(py, PyReject::from_core(reject))?
                    .into_any()
                    .unbind())
            })
            .transpose()
    }

    fn __repr__(&self) -> String {
        format!("DecodeProblem({})", self.inner)
    }
}

// ── encode() / decode() ─────────────────────────────────────────────────────
/// Encode any TCAP message object (Begin / Continue / End / Abort /
/// Unidirectional) to BER `bytes`. Equivalent to calling `msg.encode()`.
#[pyfunction]
fn encode<'py>(py: Python<'py>, message: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    if let Ok(m) = message.cast::<PyBegin>() {
        m.borrow().encode(py)
    } else if let Ok(m) = message.cast::<PyContinue>() {
        m.borrow().encode(py)
    } else if let Ok(m) = message.cast::<PyEnd>() {
        m.borrow().encode(py)
    } else if let Ok(m) = message.cast::<PyAbort>() {
        m.borrow().encode(py)
    } else if let Ok(m) = message.cast::<PyUnidirectional>() {
        m.borrow().encode(py)
    } else {
        Err(TcapError::new_err(
            "message must be a Begin, Continue, End, Abort, or Unidirectional",
        ))
    }
}

/// The Python message object for a core message.
fn message_to_py(py: Python<'_>, msg: TcapMessage) -> PyResult<Py<PyAny>> {
    let any = match msg {
        TcapMessage::Begin(b) => {
            let components = components_to_py(py, b.components.unwrap_or_default())?;
            Bound::new(
                py,
                PyBegin {
                    otid: b.otid.to_vec(),
                    dialogue_portion: b.dialogue_portion.map(|d| d.external.as_bytes().to_vec()),
                    components,
                },
            )?
            .into_any()
        }
        TcapMessage::Continue(c) => {
            let components = components_to_py(py, c.components.unwrap_or_default())?;
            Bound::new(
                py,
                PyContinue {
                    otid: c.otid.to_vec(),
                    dtid: c.dtid.to_vec(),
                    dialogue_portion: c.dialogue_portion.map(|d| d.external.as_bytes().to_vec()),
                    components,
                },
            )?
            .into_any()
        }
        TcapMessage::End(e) => {
            let components = components_to_py(py, e.components.unwrap_or_default())?;
            Bound::new(
                py,
                PyEnd {
                    dtid: e.dtid.to_vec(),
                    dialogue_portion: e.dialogue_portion.map(|d| d.external.as_bytes().to_vec()),
                    components,
                },
            )?
            .into_any()
        }
        TcapMessage::Abort(a) => Bound::new(py, PyAbort::from_core(a))?.into_any(),
        TcapMessage::Unidirectional(u) => {
            let components = components_to_py(py, u.components)?;
            Bound::new(
                py,
                PyUnidirectional {
                    dialogue_portion: u.dialogue_portion.map(|d| d.external.as_bytes().to_vec()),
                    components,
                },
            )?
            .into_any()
        }
    };
    Ok(any.unbind())
}

/// Decode a TCAP message from BER `bytes`, returning the matching message class
/// (`Begin`, `Continue`, `End`, `Abort`, or `Unidirectional`).
///
/// The message has to be fully valid. When any part of it was not understood,
/// `TcapError` is raised and its `problem` attribute holds the `DecodeProblem`;
/// a message is never returned without something that was on the wire.
#[pyfunction]
fn decode(py: Python<'_>, data: &[u8]) -> PyResult<Py<PyAny>> {
    match crate::decode_detailed(data) {
        Decoded::Complete(message) => message_to_py(py, message),
        Decoded::Problem(problem) => Err(malformed_err(py, *problem)),
    }
}

/// Decode a TCAP message from BER `bytes` without raising: returns the message
/// object when it was fully understood, and a `DecodeProblem` otherwise.
#[pyfunction]
fn decode_detailed(py: Python<'_>, data: &[u8]) -> PyResult<Py<PyAny>> {
    match crate::decode_detailed(data) {
        Decoded::Complete(message) => message_to_py(py, message),
        Decoded::Problem(problem) => Ok(Bound::new(py, PyDecodeProblem { inner: *problem })?
            .into_any()
            .unbind()),
    }
}

// ── Module wiring ───────────────────────────────────────────────────────────
fn add_contents(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("TcapError", m.py().get_type::<TcapError>())?;

    // Operation / error code helpers.
    m.add_class::<PyOperationCode>()?;
    m.add_class::<PyErrorCode>()?;

    // Components.
    m.add_class::<PyInvoke>()?;
    m.add_class::<PyReturnResult>()?;
    m.add_class::<PyReturnError>()?;
    m.add_class::<PyReject>()?;

    // Transaction messages.
    m.add_class::<PyBegin>()?;
    m.add_class::<PyContinue>()?;
    m.add_class::<PyEnd>()?;
    m.add_class::<PyAbort>()?;
    m.add_class::<PyUnidirectional>()?;

    // Dialogue portion (AARQ / AARE / ABRT).
    m.add_class::<PyDialoguePdu>()?;
    m.add_function(wrap_pyfunction!(dialogue_aarq, m)?)?;
    m.add_function(wrap_pyfunction!(dialogue_aare_accept, m)?)?;
    m.add_function(wrap_pyfunction!(dialogue_aare_reject, m)?)?;
    m.add_function(wrap_pyfunction!(dialogue_abrt, m)?)?;
    m.add_function(wrap_pyfunction!(dialogue_audt, m)?)?;
    m.add_function(wrap_pyfunction!(parse_dialogue_portion, m)?)?;
    m.add("ABORT_SOURCE_USER", ABORT_SOURCE_USER)?;
    m.add("ABORT_SOURCE_PROVIDER", ABORT_SOURCE_PROVIDER)?;

    // Codec.
    m.add_function(wrap_pyfunction!(encode, m)?)?;
    m.add_function(wrap_pyfunction!(decode, m)?)?;
    m.add_function(wrap_pyfunction!(decode_detailed, m)?)?;
    m.add_class::<PyDecodeProblem>()?;

    // P-Abort causes (Q.773 Table 12).
    m.add(
        "P_ABORT_UNRECOGNIZED_MESSAGE_TYPE",
        P_ABORT_UNRECOGNIZED_MESSAGE_TYPE,
    )?;
    m.add(
        "P_ABORT_UNRECOGNIZED_TRANSACTION_ID",
        P_ABORT_UNRECOGNIZED_TRANSACTION_ID,
    )?;
    m.add(
        "P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION",
        P_ABORT_BADLY_FORMATTED_TRANSACTION_PORTION,
    )?;
    m.add(
        "P_ABORT_INCORRECT_TRANSACTION_PORTION",
        P_ABORT_INCORRECT_TRANSACTION_PORTION,
    )?;
    m.add("P_ABORT_RESOURCE_LIMITATION", P_ABORT_RESOURCE_LIMITATION)?;

    // Reject problem classes (Q.773 Table 25) and the general problems the
    // component sub-layer raises (Table 26).
    m.add("PROBLEM_GENERAL", PROBLEM_GENERAL)?;
    m.add("PROBLEM_INVOKE", PROBLEM_INVOKE)?;
    m.add("PROBLEM_RETURN_RESULT", PROBLEM_RETURN_RESULT)?;
    m.add("PROBLEM_RETURN_ERROR", PROBLEM_RETURN_ERROR)?;
    m.add(
        "GENERAL_PROBLEM_UNRECOGNIZED_COMPONENT",
        GeneralProblem::UNRECOGNIZED_COMPONENT.value(),
    )?;
    m.add(
        "GENERAL_PROBLEM_MISTYPED_COMPONENT",
        GeneralProblem::MISTYPED_COMPONENT.value(),
    )?;
    m.add(
        "GENERAL_PROBLEM_BADLY_STRUCTURED_COMPONENT",
        GeneralProblem::BADLY_STRUCTURED_COMPONENT.value(),
    )?;

    // Q.773 transaction PDU tags (the first BER byte of an encoded message).
    m.add("TAG_UNIDIRECTIONAL", TAG_UNIDIRECTIONAL)?;
    m.add("TAG_BEGIN", TAG_BEGIN)?;
    m.add("TAG_END", TAG_END)?;
    m.add("TAG_CONTINUE", TAG_CONTINUE)?;
    m.add("TAG_ABORT", TAG_ABORT)?;

    // Component type numbers (Q.773 §3.2, CONTEXT class).
    m.add("COMPONENT_INVOKE", COMPONENT_INVOKE)?;
    m.add("COMPONENT_RETURN_RESULT_LAST", COMPONENT_RETURN_RESULT_LAST)?;
    m.add("COMPONENT_RETURN_ERROR", COMPONENT_RETURN_ERROR)?;
    m.add("COMPONENT_REJECT", COMPONENT_REJECT)?;
    m.add(
        "COMPONENT_RETURN_RESULT_NOT_LAST",
        COMPONENT_RETURN_RESULT_NOT_LAST,
    )?;

    Ok(())
}

/// Standalone wheel entry point (maturin `module-name = "tcap._tcap"`).
#[pymodule]
fn _tcap(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_contents(m)
}

/// Embedding entry point: build a `tcap` submodule and attach it to `parent`,
/// so a host extension can expose tcap without a second shared object.
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(py, "tcap")?;
    add_contents(&m)?;
    parent.setattr("tcap", &m)?;
    Ok(())
}
