// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Controlling-terminal acquisition.
//!
//! Owns the `/dev/tty` open dance and `isatty` checks (see `PLAN_04`
//! §2 and subtask 04.2). Stub today; real implementation lands in
//! 04.2.

// The `libc` crate is part of this crate's manifest as of 04.1 in
// preparation for the `/dev/tty` and `isatty` calls in 04.2. Pin it
// here so `cargo-machete` does not flag it as unused while the
// implementation is still pending.
use libc as _;
