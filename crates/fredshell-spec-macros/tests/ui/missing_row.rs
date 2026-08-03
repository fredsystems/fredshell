// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

use fredshell_spec_macros::refuse;

fn main() {
    // Row 3.99 does not exist in the fixture cd sheet.
    let _ = refuse!(wontfix, "cd", "3.99");
}
