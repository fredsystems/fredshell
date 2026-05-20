// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Kitty keyboard protocol push / pop / set encoders.
//!
//! Populated by `PLAN_03` subtask 03.5. Distinct from [`super::mode`]
//! because the kitty keyboard protocol uses push/pop semantics
//! (`CSI > flags ; mode u`) rather than DECSET/DECRST.
