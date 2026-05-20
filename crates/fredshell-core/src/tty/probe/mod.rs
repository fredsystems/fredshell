// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Individual capability probes.
//!
//! Owns the small set of one-shot queries fredshell issues at
//! startup to detect terminal features: DA1, DSR cursor position,
//! kitty keyboard query, OSC 52 query, mode-2026 query. See
//! `PLAN_04` §5 and subtask 04.3 (pure decoders) / 04.9 (I/O
//! orchestration). Stub today; decoders land in 04.3.
