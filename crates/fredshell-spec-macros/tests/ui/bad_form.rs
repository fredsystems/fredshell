// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

use fredshell_spec_macros::refuse;

fn main() {
    // `maybe` is not a valid refusal form.
    let _ = refuse!(maybe, "cd", "3.1");
}
