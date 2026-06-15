use std::fs;
use std::path::Path;

const PLACEHOLDER_INDEX: &str = include_str!("index.html");

fn main() {
    // The frontend lives at the repo root in `dashboard/`; resolve it relative
    // to this crate (CARGO_MANIFEST_DIR = crates/larkstack).
    let dist = Path::new("..").join("..").join("dashboard").join("dist");
    if !dist.join("index.html").exists() {
        let _ = fs::create_dir_all(&dist);
        let _ = fs::write(dist.join("index.html"), PLACEHOLDER_INDEX);
    }
    println!("cargo:rerun-if-changed=../../dashboard/dist");
    println!("cargo:rerun-if-changed=index.html");
}
