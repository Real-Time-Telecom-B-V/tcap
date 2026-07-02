//! Memory-leak check.
//!
//! A counting global allocator tracks **live bytes** (allocated − freed) — RSS
//! is too noisy (the OS/allocator retains freed pages), but live bytes are
//! exact, so a real leak shows up as monotonic growth. Two phases, each a
//! representative TCAP transaction encoded then decoded, over and over:
//!
//!   1. **begin/invoke** — a Begin carrying an Invoke with an opaque operation
//!      argument (the opening leg of a MAP-style dialogue).
//!   2. **end/return-result** — an End carrying a ReturnResultLast (the closing
//!      leg).
//!
//! Each phase asserts live bytes return to a flat baseline. Exits non-zero on a
//! leak. Driven by `scripts/mem_leak_test.sh`.
//!
//! Run: `cargo run --release --example leak_check`

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};

use tcap::{
    Begin, Component, End, Invoke, OperationCode, ReturnResult, ReturnResultValue, TcapMessage,
};

// ── Counting allocator ──────────────────────────────────────────────────────
static LIVE: AtomicI64 = AtomicI64::new(0);

struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l);
        LIVE.fetch_sub(l.size() as i64, Ordering::Relaxed);
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, ptr: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, l, new_size);
        if !p.is_null() {
            LIVE.fetch_add(new_size as i64 - l.size() as i64, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> i64 {
    LIVE.load(Ordering::Relaxed)
}

// ── Fixtures ────────────────────────────────────────────────────────────────
fn sample_parameter() -> rasn::types::Any {
    let mut body = vec![0x04, 0x20]; // OCTET STRING, len 32 (synthetic)
    body.extend_from_slice(&[0xAB; 32]);
    rasn::types::Any::new(body)
}

fn begin_with_invoke() -> TcapMessage {
    TcapMessage::Begin(Begin {
        otid: vec![0x00, 0x00, 0x00, 0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Invoke(Invoke {
            invoke_id: 1,
            linked_id: None,
            operation_code: OperationCode::Local(45),
            parameter: Some(sample_parameter()),
        })]),
    })
}

fn end_with_return_result() -> TcapMessage {
    TcapMessage::End(End {
        dtid: vec![0x00, 0x00, 0x00, 0x02].into(),
        dialogue_portion: None,
        components: Some(vec![Component::ReturnResultLast(ReturnResult {
            invoke_id: 1,
            result: Some(ReturnResultValue {
                operation_code: OperationCode::Local(45),
                parameter: Some(sample_parameter()),
            }),
        })]),
    })
}

// ── Phases: encode + decode round-trips ─────────────────────────────────────
fn codec_cycle(msg: &TcapMessage, iters: usize) {
    for _ in 0..iters {
        let wire = tcap::encode(msg).unwrap();
        std::hint::black_box(tcap::decode(&wire).unwrap());
    }
}

fn report(phase: &str, base: i64) -> i64 {
    let growth = live() - base;
    println!("  {phase}: live = {} bytes (Δ {:+})", live(), growth);
    growth
}

fn run_phase(name: &str, msg: &TcapMessage, iters: usize, cycles: usize) -> i64 {
    println!("[{name}] {cycles} x {iters} encode+decode round-trips");
    codec_cycle(msg, iters); // warm up
    let base = live();
    for c in 1..=cycles {
        codec_cycle(msg, iters);
        report(&format!("cycle {c:>2}/{cycles}"), base);
    }
    live() - base
}

fn main() {
    const ITERS: usize = 200_000;
    const CYCLES: usize = 10;
    const BUDGET: i64 = 64 * 1024;

    let begin = begin_with_invoke();
    let end = end_with_return_result();

    let begin_growth = run_phase("begin/invoke", &begin, ITERS, CYCLES);
    println!();
    let end_growth = run_phase("end/return-result", &end, ITERS, CYCLES);

    // Verdict.
    println!();
    let mut ok = true;
    if begin_growth > BUDGET {
        eprintln!("FAIL: begin/invoke live bytes grew {begin_growth} (> {BUDGET})");
        ok = false;
    }
    if end_growth > BUDGET {
        eprintln!("FAIL: end/return-result live bytes grew {end_growth} (> {BUDGET})");
        ok = false;
    }
    if !ok {
        std::process::exit(1);
    }
    println!(
        "PASS: begin/invoke Δ {begin_growth} ≤ {BUDGET}; end/return-result Δ {end_growth} ≤ {BUDGET}"
    );
}
