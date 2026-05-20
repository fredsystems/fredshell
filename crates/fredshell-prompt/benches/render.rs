// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Microbenchmarks for [`fredshell_prompt::render`].
//!
//! The prompt is rendered before every interactive command line,
//! so its latency directly affects perceived shell responsiveness
//! (`PLAN_02` §9). Two scenarios are exercised: a typical
//! happy-path render with `last_status = 0`, and the failing-status
//! variant that swaps the arrow color.
//!
//! Paths used here are synthetic and lexical-only — `render` calls
//! `file_name()` and `display()`, never the filesystem — so the
//! bench is reproducible on any host (developer workstation or CI
//! runner).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use fredshell_prompt::{PromptConfig, PromptContext, render};

fn bench_render_success(c: &mut Criterion) {
    let cfg = PromptConfig::default();
    let ctx = PromptContext {
        cwd: PathBuf::from("/workspace/project"),
        last_status: 0,
    };
    c.bench_function("prompt_render_success", |b| {
        b.iter(|| render(black_box(&cfg), black_box(&ctx)));
    });
}

fn bench_render_failure(c: &mut Criterion) {
    let cfg = PromptConfig::default();
    let ctx = PromptContext {
        cwd: PathBuf::from("/workspace/project"),
        last_status: 1,
    };
    c.bench_function("prompt_render_failure", |b| {
        b.iter(|| render(black_box(&cfg), black_box(&ctx)));
    });
}

criterion_group!(benches, bench_render_success, bench_render_failure);
criterion_main!(benches);
