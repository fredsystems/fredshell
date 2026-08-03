// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

use fredshell_spec_macros::refuse;

fn main() {
    // Row 3.9 is classified `defer:3`; the `wontfix` form must reject it.
    let _ = refuse!(wontfix, "cd", "3.9");
}
