use fredshell_spec_macros::refuse;

fn main() {
    // `maybe` is not a valid refusal form.
    let _ = refuse!(maybe, "cd", "3.1");
}
