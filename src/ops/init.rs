use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::RADIAL_DIR;
use crate::db::Database;

/// Where `.radial` got added for git exclusion, if it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitignoreTarget {
    Exclude,
    Gitignore,
}

impl GitignoreTarget {
    pub fn display_path(self) -> &'static str {
        match self {
            GitignoreTarget::Exclude => ".git/info/exclude",
            GitignoreTarget::Gitignore => ".gitignore",
        }
    }
}

#[derive(Debug)]
pub struct InitResult {
    pub radial_dir: PathBuf,
    pub already_initialized: bool,
    pub gitignore_target: Option<GitignoreTarget>,
}

pub fn run(stealth: bool) -> Result<InitResult> {
    let radial_dir = PathBuf::from(RADIAL_DIR);

    if radial_dir.exists() {
        return Ok(InitResult {
            radial_dir,
            already_initialized: true,
            gitignore_target: None,
        });
    }

    fs::create_dir_all(&radial_dir).context("Failed to create .radial directory")?;

    let db = Database::open(&radial_dir)?;
    db.init_schema()?;

    let gitignore_target = if stealth { add_to_gitignore()? } else { None };

    Ok(InitResult {
        radial_dir,
        already_initialized: false,
        gitignore_target,
    })
}

/// Adds `.radial` to git exclusions.
/// Prefers `.git/info/exclude` if it exists (truly local), otherwise uses `.gitignore`.
/// Returns `None` if `.radial` was already excluded or we're not in a git repo.
fn add_to_gitignore() -> Result<Option<GitignoreTarget>> {
    let exclude_path = Path::new(".git/info/exclude");
    let gitignore_path = Path::new(".gitignore");

    // Prefer .git/info/exclude for truly local exclusion
    let (target_path, target) = if exclude_path.exists() {
        (exclude_path, GitignoreTarget::Exclude)
    } else if gitignore_path.exists() || Path::new(".git").is_dir() {
        // If we're in a git repo, create/use .gitignore
        (gitignore_path, GitignoreTarget::Gitignore)
    } else {
        // Not a git repo, skip
        return Ok(None);
    };

    // Check if already excluded
    if target_path.exists() {
        let content = fs::read_to_string(target_path).unwrap_or_default();
        let has_radial = content
            .lines()
            .any(|line| line.trim() == ".radial" || line.trim() == ".radial/");
        if has_radial {
            return Ok(None);
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

    Ok(Some(target))
}
