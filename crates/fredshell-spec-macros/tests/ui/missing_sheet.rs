use fredshell_spec_macros::refuse;

fn main() {
    // There is no fixture sheet named `nonesuch`.
    let _ = refuse!(wontfix, "nonesuch", "3.1");
}
