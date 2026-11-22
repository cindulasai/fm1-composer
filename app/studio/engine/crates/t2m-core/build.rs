//! Rebuild when the embedded knowledge base changes.

fn main() {
    println!("cargo:rerun-if-changed=data");
}
