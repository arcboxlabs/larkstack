use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const LINEAR_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/linear/linear/master/packages/sdk/src/schema.graphql";

pub fn update_linear_schema() -> Result<()> {
    let root = workspace_root();
    let dest = root.join("apps/integrations/linear/graphql/schema.graphql");
    let body = fetch(LINEAR_SCHEMA_URL)?;
    let schema = format!(
        "# Linear GraphQL schema — generated, DO NOT EDIT BY HAND.\n\
         # Source: https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql\n\
         # Refresh: run `cargo xtask update-linear-schema`, then commit.\n\n{body}"
    );

    write_atomic(&dest, schema.as_bytes())?;
    let line_count = schema.lines().count();
    println!("updated {} ({line_count} lines)", dest.display());
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live directly under the workspace root")
        .to_path_buf()
}

fn fetch(url: &str) -> Result<String> {
    ureq::get(url)
        .call()
        .with_context(|| format!("failed to fetch {url}"))?
        .into_string()
        .with_context(|| format!("failed to read response from {url}"))
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temporary file in {}", parent.display()))?;
    std::io::Write::write_all(&mut tmp, contents)
        .with_context(|| format!("failed to write temporary file for {}", path.display()))?;
    tmp.persist(path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}
