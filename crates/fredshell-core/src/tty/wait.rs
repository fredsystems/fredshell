// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `pselect`/`poll` multiplexer for terminal input and signals.
//!
//! Owns the body of [`super::TerminalSession::wait`] (see `PLAN_04`
//! §6 and subtask 04.6) plus the `TtyInput` / `TtyOutput` byte
//! channels built on top of it. Stub today.
