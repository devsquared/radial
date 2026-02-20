use std::io::{self, Write};

use anyhow::Result;
use console::style;

use crate::db::Database;
use crate::models::GoalState;

pub fn run(all: bool, force: bool, db: &mut Database) -> Result<()> {
    let goals: Vec<_> = db
        .list_goals()
        .into_iter()
        .filter(|g| force || g.state() == GoalState::Completed)
        .cloned()
        .collect();

    if goals.is_empty() {
        let msg = if force {
            "No goals found."
        } else {
            "No completed goals to clean."
        };
        println!("{msg}");
        return Ok(());
    }

    let mut removed = 0;

    for goal in &goals {
        // --all or --force skip prompting
        let should_remove = all || force || prompt_for_goal(goal)?;

        if should_remove {
            db.delete_goal(goal.id())?;
            println!(
                "  {} {} — {}",
                style("Removed").red(),
                style(goal.id()).cyan(),
                truncate(goal.description(), 60),
            );
            removed += 1;
        }
    }

    println!(
        "\nCleaned {} goal(s).",
        style(removed).bold()
    );
    Ok(())
}

/// Prompt the user to confirm deletion of a single goal.
fn prompt_for_goal(goal: &crate::models::Goal) -> Result<bool> {
    let mut stdout = io::stdout().lock();
    write!(
        stdout,
        "Remove {} [{}] {}? [y/N] ",
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
