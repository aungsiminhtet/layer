use crate::exclude_file::ensure_exclude_file_for_write;
use crate::git;
use anyhow::{anyhow, Context, Result};
use std::env;
use std::process::Command;

pub fn run() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let _exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;

    let editor = env::var("VISUAL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env::var("EDITOR").ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "vi".to_string());

    println!("Opening .git/info/exclude in {editor}...");

    let status = Command::new(&editor)
        .arg(&ctx.exclude_path)
        .status()
        .with_context(|| format!("failed to launch editor '{editor}'"))?;

    if !status.success() {
        return Err(anyhow!("editor exited with status {status}"));
    }

    Ok(0)
}
