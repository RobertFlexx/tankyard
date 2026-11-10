// Tank Yard: Cargo fingerprints only this crate's Rust files, so without this
// an edit to the vendored Luau sources would not rebuild mlua-sys.
fn main() {
    println!("cargo:rerun-if-changed=luau");
}
