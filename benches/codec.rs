//! Codec micro-benchmarks: TCAP message encode/decode over BER.
//!
//! Run with `cargo bench`. Numbers feed the README "Performance" table.
//!
//! Two representative transactions, built from the public API (synthetic /
//! spec-derived values, no captured traffic): a **Begin carrying an Invoke** (the
//! opening leg of a MAP-style dialogue) and an **End carrying a ReturnResult**
//! (the closing leg). Each is benched for both encode and decode, so the numbers
//! isolate exactly the BER work this crate does — no I/O in the path.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use tcap::{
    Begin, Component, End, Invoke, OperationCode, ReturnResult, ReturnResultValue, TcapMessage,
};

/// A synthetic operation argument (an opaque OCTET STRING) — length is what
/// matters for the copy path, not the contents.
fn sample_parameter() -> rasn::types::Any {
    let mut body = vec![0x04, 0x20]; // OCTET STRING, len 32
    body.extend_from_slice(&[0xAB; 32]);
    rasn::types::Any::new(body)
}

fn begin_with_invoke() -> TcapMessage {
    let invoke = Invoke {
        invoke_id: 1,
        linked_id: None,
        operation_code: OperationCode::Local(45),
        parameter: Some(sample_parameter()),
    };
    TcapMessage::Begin(Begin {
        otid: vec![0x00, 0x00, 0x00, 0x01].into(),
        dialogue_portion: None,
        components: Some(vec![Component::Invoke(invoke)]),
    })
}

fn end_with_return_result() -> TcapMessage {
    let rr = ReturnResult {
        invoke_id: 1,
        result: Some(ReturnResultValue {
            operation_code: OperationCode::Local(45),
            parameter: Some(sample_parameter()),
        }),
    };
    TcapMessage::End(End {
        dtid: vec![0x00, 0x00, 0x00, 0x02].into(),
        dialogue_portion: None,
        components: Some(vec![Component::ReturnResultLast(rr)]),
    })
}

fn bench_codec(c: &mut Criterion) {
    let begin = begin_with_invoke();
    let end = end_with_return_result();
    let begin_bytes = tcap::encode(&begin).expect("valid begin");
    let end_bytes = tcap::encode(&end).expect("valid end");

    let mut g = c.benchmark_group("codec");
    g.throughput(Throughput::Elements(1));

    g.bench_function("begin_invoke/encode", |b| {
        b.iter_batched(
            || begin.clone(),
            |m| tcap::encode(&m).unwrap(),
            BatchSize::SmallInput,
        )
    });
    g.bench_function("begin_invoke/decode", |b| {
        b.iter(|| tcap::decode(&begin_bytes).unwrap())
    });
    g.bench_function("end_return_result/encode", |b| {
        b.iter_batched(
            || end.clone(),
            |m| tcap::encode(&m).unwrap(),
            BatchSize::SmallInput,
        )
    });
    g.bench_function("end_return_result/decode", |b| {
        b.iter(|| tcap::decode(&end_bytes).unwrap())
    });
    g.finish();
}

criterion_group!(benches, bench_codec);
criterion_main!(benches);
