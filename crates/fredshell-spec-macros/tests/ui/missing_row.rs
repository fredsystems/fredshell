use fredshell_spec_macros::refuse;

fn main() {
    // Row 3.99 does not exist in the fixture cd sheet.
    let _ = refuse!(wontfix, "cd", "3.99");
}
