// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Process-group plumbing for job control.
//!
//! Owns `setpgid` / `tcsetpgrp` helpers and the `Pid` newtype used
//! when handing the terminal foreground to child process groups
//! (see `PLAN_04` §7 and subtask 04.8). Stub today.
