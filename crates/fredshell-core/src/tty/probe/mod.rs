// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Capability probes.
//!
//! This module owns three responsibilities, all pure (no I/O, no
//! syscalls): given decoded response structs and an environment
//! snapshot, produce a [`Capabilities`] aggregate. The actual I/O
//! orchestration (write the probe batch, read responses with a 50
//! ms budget) lives in [`super::capabilities`] and lands in subtask
//! 04.9.
//!
//! Three information sources, in priority order (`PLAN_04` §5.2):
//!
//! 1. **Active probe responses** ([`interpret`]). Highest signal:
//!    the terminal explicitly answered.
//! 2. **Environment-variable heuristics** ([`env`]). Free,
//!    synchronous, and reliable for the features that lack a probe
//!    (OSC 8, OSC 7, OSC 133, bracketed paste, focus reporting).
//! 3. **Conservative defaults** ([`Capabilities::default`]).
//!
//! [`Capabilities`]: super::capabilities::Capabilities
//! [`Capabilities::default`]: super::capabilities::Capabilities::default

pub mod env;
pub mod interpret;
pub mod run;
