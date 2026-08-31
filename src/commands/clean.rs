use std::io::{self, Write};

use anyhow::Result;
use console::style;

use crate::db::Database;
use crate::models::GoalState;

pub fn run(all: bool, force: bool, purge: bool, db: &mut Database) -> Result<()> {
    let goals: Vec<_> = db
        .list_goals()
        .into_iter()
        .filter(|g| force || g.state() == GoalState::Completed || g.state() == GoalState::Cancelled)
        .cloned()
        .collect();

    if goals.is_empty() {
        let msg = if force {
            "No goals found."
        } else {
            "No completed or cancelled goals to clean."
        };
        println!("{msg}");
        return Ok(());
    }

    let mut removed = 0;

    for goal in &goals {
        // --all or --force skip prompting
        let should_remove = all || force || prompt_for_goal(goal, purge)?;

        if should_remove {
            if purge {
                db.delete_goal(goal.id())?;
                println!(
                    "  {} {} — {}",
                    style("Deleted").red(),
                    style(goal.id()).cyan(),
                    truncate(goal.description(), 60),
                );
            } else {
                db.archive_goal(goal.id())?;
                println!(
                    "  {} {} — {}",
                    style("Archived").dim(),
                    style(goal.id()).cyan(),
                    truncate(goal.description(), 60),
                );
            }
            removed += 1;
        }
    }

    let action = if purge { "Deleted" } else { "Archived" };
    println!("\n{} {} goal(s).", action, style(removed).bold());
    Ok(())
}

/// Prompt the user to confirm archiving or deletion of a single goal.
fn prompt_for_goal(goal: &crate::models::Goal, purge: bool) -> Result<bool> {
    let mut stdout = io::stdout().lock();
    let action = if purge { "Delete" } else { "Archive" };
    write!(
        stdout,
        "{} {} [{}] {}? [y/N] ",
        action,
        style(goal.id()).cyan().bold(),
        style(goal.state().as_ref()).dim(),
        truncate(goal.description(), 50),
    )?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

fn truncate(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}…", &first_line[..max - 1])
    }
}
