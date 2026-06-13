use std::fs;
use std::path::Path;

fn main() {
    // The frontend lives at the repo root in `dashboard/`; resolve it relative
    // to this crate (CARGO_MANIFEST_DIR = crates/larkstack).
    let dist = Path::new("..").join("..").join("dashboard").join("dist");
    if !dist.join("index.html").exists() {
        let _ = fs::create_dir_all(&dist);
        let placeholder = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>larkstack console</title></head>
<body><p>Frontend not built. Run <code>npm install && npm run build</code> in
<code>dashboard/</code>, then rebuild this binary.</p></body></html>
"#;
        let _ = fs::write(dist.join("index.html"), placeholder);
    }
    println!("cargo:rerun-if-changed=../../dashboard/dist");
}
