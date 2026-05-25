use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new("web").join("dist");
    if !dist.join("index.html").exists() {
        let _ = fs::create_dir_all(&dist);
        let placeholder = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>larkstack console</title></head>
<body><p>Frontend not built. Run <code>npm install && npm run build</code> in
<code>crates/console/web</code>, then rebuild this binary.</p></body></html>
"#;
        let _ = fs::write(dist.join("index.html"), placeholder);
    }
    println!("cargo:rerun-if-changed=web/dist");
}
