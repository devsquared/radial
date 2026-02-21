use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::RADIAL_DIR;
use crate::db::Database;

pub fn run(stealth: bool) -> Result<()> {
    let radial_dir = std::path::PathBuf::from(RADIAL_DIR);

    if radial_dir.exists() {
        println!("Radial already initialized in {}", radial_dir.display());
        return Ok(());
    }

    fs::create_dir_all(&radial_dir).context("Failed to create .radial directory")?;

    let db = Database::open(&radial_dir)?;
    db.init_schema()?;

    if stealth {
        add_to_gitignore()?;
    }

    println!("Initialized radial in {}", radial_dir.display());
    Ok(())
}

/// Adds `.radial` to git exclusions.
/// Prefers `.git/info/exclude` if it exists (truly local), otherwise uses `.gitignore`.
fn add_to_gitignore() -> Result<()> {
    let exclude_path = Path::new(".git/info/exclude");
    let gitignore_path = Path::new(".gitignore");

    // Prefer .git/info/exclude for truly local exclusion
    let target_path = if exclude_path.exists() {
        exclude_path
    } else if gitignore_path.exists() || Path::new(".git").is_dir() {
        // If we're in a git repo, create/use .gitignore
        gitignore_path
    } else {
        // Not a git repo, skip
        return Ok(());
    };

    // Check if already excluded
    if target_path.exists() {
        let content = fs::read_to_string(target_path).unwrap_or_default();
        if content
            .lines()
            .any(|line| line.trim() == ".radial" || line.trim() == ".radial/")
        {
            return Ok(());
        }
    }

    // Append .radial to the exclusion file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(target_path)
        .context("Failed to open git exclusion file")?;

    // Add newline if file doesn't end with one
    if target_path.exists() {
        let content = fs::read_to_string(target_path).unwrap_or_default();
        if !content.is_empty() && !content.ends_with('\n') {
            writeln!(file)?;
        }
    }

    writeln!(file, ".radial")?;

    let path_display = if target_path == exclude_path {
        ".git/info/exclude"
    } else {
        ".gitignore"
    };
    println!("Added .radial to {path_display}");

    Ok(())
}
