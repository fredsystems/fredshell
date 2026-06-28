use fredshell_spec_macros::refuse;

fn main() {
    // Row 3.9 is classified `defer:3`; the `wontfix` form must reject it.
    let _ = refuse!(wontfix, "cd", "3.9");
}
