// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Microbenchmarks for the [`PLAN_06a`] execution pipeline.
//!
//! Seeds the `PLAN_02` §9 budget tracker with a baseline for the
//! 06a stub dispatcher so `PLAN_06b` (native parser + executor) has
//! a "before" data point to measure against.
//!
//! Two scenarios are exercised:
//!
//! * `parse_only` — calls [`fredshell_core::parse`] on the literal
//!   `"true"`. In v0 the parser stores the source verbatim, so this
//!   should approach noise; the bench exists so 06b's real parser
//!   has a known starting point.
//! * `parse_and_exec` — calls [`fredshell_core::run_source`] on the
//!   same input, which routes through the stub dispatcher and ends
//!   in `/bin/sh -c true`. Latency is therefore bounded by
//!   `fork + execve`, on the order of milliseconds. This is the
//!   ceiling 06b must drop well below by executing `true` as a
//!   Tier-1 builtin instead of spawning a shell.
//!
//! The bench uses [`ExecEnv::sandboxed`] rooted at the system temp
//! directory so it does not mutate the process cwd or environment.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use fredshell_core::{ExecEnv, ExternalCommandPolicy, parse, run_source};

fn bench_parse_only(c: &mut Criterion) {
    c.bench_function("exec_roundtrip_parse_only", |b| {
        b.iter(|| parse(black_box("true")).expect("stub parse never fails on plain input"));
    });
}

fn bench_parse_and_exec(c: &mut Criterion) {
    c.bench_function("exec_roundtrip_parse_and_exec", |b| {
        b.iter(|| {
            let mut env = ExecEnv::sandboxed(std::env::temp_dir());
            // PLAN_05 §4.2 made `sandboxed()` strict by default; this
            // bench measures the v0 fallback-to-sh path explicitly.
            env.external_command_policy = ExternalCommandPolicy::FallbackToSh;
            run_source(black_box("true"), &mut env).expect("run_source executes `true`");
        });
    });
}

criterion_group!(benches, bench_parse_only, bench_parse_and_exec);
criterion_main!(benches);
