// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Encoder benches per `PLAN_03` §6.
//!
//! Three scenarios:
//!
//! - `sgr_encode_simple` — emit `Sgr::RESET.with_bold()` (the
//!   minimum-cost SGR write the line editor performs).
//! - `sgr_encode_complex` — emit a full style with truecolor
//!   foreground and background plus three boolean attributes.
//! - `redraw_frame` — emit the per-keystroke redraw frame
//!   (cursor goto, erase to end of line, SGR set, content, SGR
//!   reset). The line editor is budgeted under 1ms per keystroke;
//!   this is a substantial fraction of that budget.

use criterion::{Criterion, criterion_group, criterion_main};
use fredshell_ansi::cursor::Cursor;
use fredshell_ansi::erase::Erase;
use fredshell_ansi::sgr::{Color, Sgr};
use fredshell_ansi::{Encode, EncodeDyn, encode_all};
use std::hint::black_box;

#[allow(clippy::expect_used)]
fn sgr_encode_simple(c: &mut Criterion) {
    let s = Sgr::RESET.with_bold();
    c.bench_function("sgr_encode_simple", |b| {
        let mut buf = Vec::with_capacity(16);
        b.iter(|| {
            buf.clear();
            black_box(&s)
                .encode(&mut buf)
                .expect("write to Vec is infallible");
            black_box(&buf);
        });
    });
}

#[allow(clippy::expect_used)]
fn sgr_encode_complex(c: &mut Criterion) {
    let s = Sgr::fg(Color::Rgb {
        r: 200,
        g: 100,
        b: 50,
    })
    .with_bg(Color::Rgb {
        r: 30,
        g: 30,
        b: 30,
    })
    .with_bold()
    .with_italic()
    .with_underline(fredshell_ansi::sgr::Underline::Single);
    c.bench_function("sgr_encode_complex", |b| {
        let mut buf = Vec::with_capacity(64);
        b.iter(|| {
            buf.clear();
            black_box(&s)
                .encode(&mut buf)
                .expect("write to Vec is infallible");
            black_box(&buf);
        });
    });
}

#[allow(clippy::expect_used)]
fn redraw_frame(c: &mut Criterion) {
    // The frame the line editor writes per keystroke:
    //   CSI 1;1H              — home cursor
    //   CSI 0K                — erase to end of line
    //   CSI 1m                — bold
    //   <user content>        — written by the caller
    //   CSI 0m                — reset
    let goto = Cursor::goto(1, 1).expect("1,1 is valid");
    let erase = Erase::InLineToEnd;
    let bold = Sgr::RESET.with_bold();
    let reset = Sgr::RESET;
    let content: &[u8] = b"$ echo hello world";

    c.bench_function("redraw_frame", |b| {
        let mut buf = Vec::with_capacity(64);
        b.iter(|| {
            buf.clear();
            let items: [&dyn EncodeDyn; 3] = [&goto, &erase, &bold];
            encode_all(&mut buf, &items).expect("write to Vec is infallible");
            buf.extend_from_slice(black_box(content));
            reset.encode(&mut buf).expect("write to Vec is infallible");
            black_box(&buf);
        });
    });
}

criterion_group!(benches, sgr_encode_simple, sgr_encode_complex, redraw_frame);
criterion_main!(benches);
