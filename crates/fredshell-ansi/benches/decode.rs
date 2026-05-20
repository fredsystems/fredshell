// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Decoder benches per `PLAN_03` §6.
//!
//! One bench per response type. Each iteration decodes a single
//! complete response from a static byte slice; the decoder is
//! pure, so the bench measures parsing cost only (no I/O, no
//! allocation for fixed-shape responses).

use criterion::{Criterion, criterion_group, criterion_main};
use fredshell_ansi::Decode;
use fredshell_ansi::decode::{
    Da1Response, DsrCursorPosition, KittyKeyboardQueryResponse, Osc52ReadResponse,
};
use std::hint::black_box;

fn da1_decode(c: &mut Criterion) {
    let input: &[u8] = b"\x1b[?64;1;2;6;9;15;22c";
    c.bench_function("da1_decode", |b| {
        b.iter(|| {
            let result = Da1Response::decode(black_box(input));
            black_box(result)
        });
    });
}

fn dsr_decode(c: &mut Criterion) {
    let input: &[u8] = b"\x1b[12;34R";
    c.bench_function("dsr_decode", |b| {
        b.iter(|| {
            let result = DsrCursorPosition::decode(black_box(input));
            black_box(result)
        });
    });
}

fn kkbd_decode(c: &mut Criterion) {
    let input: &[u8] = b"\x1b[?31u";
    c.bench_function("kkbd_decode", |b| {
        b.iter(|| {
            let result = KittyKeyboardQueryResponse::decode(black_box(input));
            black_box(result)
        });
    });
}

fn osc52_decode(c: &mut Criterion) {
    // base64("the quick brown fox jumps over the lazy dog") = "dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw=="
    let input: &[u8] =
        b"\x1b]52;c;dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw==\x1b\\";
    c.bench_function("osc52_decode", |b| {
        b.iter(|| {
            let result = Osc52ReadResponse::decode(black_box(input));
            black_box(result)
        });
    });
}

criterion_group!(benches, da1_decode, dsr_decode, kkbd_decode, osc52_decode);
criterion_main!(benches);
