use fredshell_spec_macros::refuse;

fn main() {
    // Row 3.1 is classified `support`, not `wontfix`.
    let _ = refuse!(wontfix, "cd", "3.1");
}
