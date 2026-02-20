use std::io::{self, Write};

use anyhow::Result;
use console::style;
use serde::Serialize;

use crate::commands::list::GoalWithTasks;
use crate::commands::show::ShowResult;
use crate::commands::status::{GoalSummary, StatusResult};
use crate::commands::task::CompleteResult;
use crate::models::{Goal, Task};

/// Trait for types that can render themselves as human-readable CLI output.
pub trait Render {
    fn render(&self, w: &mut dyn Write) -> Result<()>;
}

/// Print as JSON if `json` is true, otherwise call `human` with a writer.
fn json_or<T: Serialize + ?Sized>(
    value: &T,
    json: bool,
    human: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut stdout, value)?;
        writeln!(stdout)?;
    } else {
        human(&mut stdout)?;
    }
    Ok(())
}

/// Truncate a string to the first line, capping at `max` characters.
fn truncate(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}…", &first_line[..max - 1])
    }
}

// -- Goal outputs --

pub fn goal_created(goal: &Goal, json: bool) -> Result<()> {
    json_or(goal, json, |w| {
        writeln!(
            w,
            "{} {}",
            style("Created goal:").green(),
            style(goal.id()).cyan().bold()
        )?;
        writeln!(w, "  {}", truncate(goal.description(), 80))?;
        Ok(())
    })
}

pub fn goal_list(goals: &[Goal], json: bool) -> Result<()> {
    json_or(goals, json, |w| {
        if goals.is_empty() {
            writeln!(w, "No goals found.")?;
            return Ok(());
        }

        // Compact columnar list
        writeln!(
            w,
            "{:<10} {:<13} {}",
            style("ID").bold().underlined(),
            style("STATE").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;
        for goal in goals {
            writeln!(
                w,
                "{:<10} {:<13} {}",
                style(goal.id()).cyan(),
                state_styled(goal.state().as_ref()),
                truncate(goal.description(), 80),
            )?;
        }
        Ok(())
    })
}

// -- Task outputs --

pub fn task_created(task: &Task, json: bool) -> Result<()> {
    json_or(task, json, |w| {
        writeln!(
            w,
            "{} {}",
            style("Created task:").green(),
            style(task.id()).cyan().bold()
        )?;
        writeln!(w, "  {}", truncate(task.description(), 80))?;
        writeln!(w, "  State: {}", state_styled(task.state().as_ref()))?;
        if task.contract().is_none() {
            writeln!(
                w,
                "  Contract: {}",
                style("(not set — required before starting)").dim()
            )?;
        }
        Ok(())
    })
}

pub fn task_list(tasks: &[Task], goal: &Goal, verbose: bool, json: bool) -> Result<()> {
    json_or(tasks, json, |w| {
        writeln!(
            w,
            "Tasks for {} [{}]",
            style(goal.id()).cyan().bold(),
            state_styled(goal.state().as_ref()),
        )?;
        writeln!(w, "  {}", truncate(goal.description(), 80))?;
        writeln!(w)?;

        if tasks.is_empty() {
            writeln!(w, "No tasks found.")?;
            return Ok(());
        }

        writeln!(
            w,
            "{:<10} {:<13} {}",
            style("ID").bold().underlined(),
            style("STATE").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;
        for task in tasks {
            writeln!(
                w,
                "{:<10} {:<13} {}",
                style(task.id()).cyan(),
                state_styled(task.state().as_ref()),
                truncate(task.description(), 80),
            )?;
            if verbose && !task.comments().is_empty() {
                for comment in task.comments() {
                    writeln!(
                        w,
                        "           {}  {}",
                        style(comment.created_at()).dim(),
                        truncate(comment.text(), 60),
                    )?;
                }
            }
        }
        Ok(())
    })
}

pub fn task_started(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Started task:").green(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    Ok(())
}

pub fn task_completed(result: &CompleteResult) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Completed task:").green(),
        style(result.task.id()).cyan().bold()
    )?;
    if let Some(res) = result.task.result() {
        writeln!(w, "  {}", truncate(res.summary(), 80))?;
    }

    if !result.unblocked_task_ids.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Unblocked tasks:").yellow())?;
        for id in &result.unblocked_task_ids {
            writeln!(w, "  - {}", style(id).cyan())?;
        }
    }
    Ok(())
}

pub fn task_failed(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Failed task:").red(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    Ok(())
}

pub fn task_retry(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Retrying task:").yellow(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    writeln!(w, "  Retry count: {}", task.metrics().retry_count())?;
    Ok(())
}

pub fn task_commented(task: &Task, json: bool) -> Result<()> {
    json_or(task, json, |w| {
        writeln!(
            w,
            "{} {}",
            style("Added comment to task:").green(),
            style(task.id()).cyan().bold()
        )?;
        if let Some(comment) = task.comments().last() {
            writeln!(w, "  {}", truncate(comment.text(), 80))?;
        }
        writeln!(w, "  Total comments: {}", task.comments().len())?;
        Ok(())
    })
}

// -- Status outputs (compact) --

pub fn status(result: &StatusResult, json: bool) -> Result<()> {
    match result {
        StatusResult::Task(task) => status_task(task, json),
        StatusResult::Goal(goal_status) => status_goal(goal_status, json),
        StatusResult::AllGoals(summaries) => status_all_goals(summaries, json),
    }
}

fn status_task(task: &Task, json: bool) -> Result<()> {
    json_or(task, json, |w| {
        writeln!(
            w,
            "{:<10} {:<13} {}",
            style(task.id()).cyan(),
            state_styled(task.state().as_ref()),
            truncate(task.description(), 80),
        )?;
        Ok(())
    })
}

fn status_goal(goal_status: &crate::commands::status::GoalStatus, json: bool) -> Result<()> {
    json_or(goal_status, json, |w| {
        let goal = goal_status.goal();
        let metrics = goal_status.metrics();

        writeln!(
            w,
            "Goal: {}  {}  ({}/{} tasks)",
            style(goal.id()).cyan().bold(),
            state_styled(goal.state().as_ref()),
            metrics.tasks_completed(),
            metrics.task_count(),
        )?;
        writeln!(w, "  {}", truncate(goal.description(), 80))?;
        writeln!(w)?;

        if !goal_status.tasks().is_empty() {
            writeln!(
                w,
                "{:<10} {:<13} {}",
                style("ID").bold().underlined(),
                style("STATE").bold().underlined(),
                style("DESCRIPTION").bold().underlined(),
            )?;
            for task in goal_status.tasks() {
                writeln!(
                    w,
                    "{:<10} {:<13} {}",
                    style(task.id()).cyan(),
                    state_styled(task.state().as_ref()),
                    truncate(task.description(), 80),
                )?;
            }
        }
        Ok(())
    })
}

fn status_all_goals(summaries: &[GoalSummary], json: bool) -> Result<()> {
    json_or(summaries, json, |w| {
        if summaries.is_empty() {
            writeln!(w, "No goals found.")?;
            return Ok(());
        }

        writeln!(
            w,
            "{:<10} {:<13} {:<7} {}",
            style("ID").bold().underlined(),
            style("STATE").bold().underlined(),
            style("TASKS").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;
        for summary in summaries {
            let goal = summary.goal();
            let metrics = summary.computed_metrics();
            writeln!(
                w,
                "{:<10} {:<13} {:<7} {}",
                style(goal.id()).cyan(),
                state_styled(goal.state().as_ref()),
                format!("{}/{}", metrics.tasks_completed(), metrics.task_count()),
                truncate(goal.description(), 80),
            )?;
        }
        Ok(())
    })
}

// -- Show outputs (full detail) --

pub fn show(result: &ShowResult, json: bool) -> Result<()> {
    match result {
        ShowResult::Task(task) => show_task(task, json),
        ShowResult::Goal {
            goal,
            tasks,
            metrics,
        } => show_goal(goal, tasks, metrics, json),
    }
}

fn show_task(task: &Task, json: bool) -> Result<()> {
    json_or(task, json, |w| {
        writeln!(
            w,
            "Task {}  [{}]",
            style(task.id()).cyan().bold(),
            state_styled(task.state().as_ref()),
        )?;
        writeln!(w)?;

        writeln!(w, "{}", style("Description").bold())?;
        for line in task.description().lines() {
            writeln!(w, "  {line}")?;
        }

        writeln!(w)?;
        field(w, "Goal", task.goal_id())?;
        field(w, "Created", &task.created_at().to_string())?;
        field(w, "Updated", &task.updated_at().to_string())?;

        // Contract
        writeln!(w)?;
        match task.contract() {
            Some(contract) => {
                writeln!(w, "{}", style("Contract").bold())?;
                field(w, "  Receives", contract.receives())?;
                field(w, "  Produces", contract.produces())?;
                field(w, "  Verify", contract.verify())?;
            }
            None => {
                writeln!(
                    w,
                    "{} {}",
                    style("Contract").bold(),
                    style("(not set)").dim()
                )?;
            }
        }

        if !task.blocked_by().is_empty() {
            writeln!(w)?;
            field(w, "Blocked by", &task.blocked_by().join(", "))?;
        }

        if let Some(result) = task.result() {
            writeln!(w)?;
            writeln!(w, "{}", style("Result").bold())?;
            for line in result.summary().lines() {
                writeln!(w, "  {line}")?;
            }
            if !result.artifacts().is_empty() {
                field(w, "  Artifacts", &result.artifacts().join(", "))?;
            }
        }

        if !task.comments().is_empty() {
            writeln!(w)?;
            writeln!(
                w,
                "{} ({})",
                style("Comments").bold(),
                task.comments().len()
            )?;
            for comment in task.comments() {
                writeln!(w, "  {}", style(format!("[{}]", comment.created_at())).dim())?;
                for line in comment.text().lines() {
                    writeln!(w, "  {line}")?;
                }
                writeln!(w)?;
            }
        }

        Ok(())
    })
}

fn show_goal(
    goal: &Goal,
    tasks: &[Task],
    metrics: &crate::models::Metrics,
    json: bool,
) -> Result<()> {
    // Wrap in a struct for JSON serialization
    #[derive(Serialize)]
    struct GoalDetail<'a> {
        #[serde(flatten)]
        goal: &'a Goal,
        tasks: &'a [Task],
        metrics: &'a crate::models::Metrics,
    }
    let detail = GoalDetail {
        goal,
        tasks,
        metrics,
    };

    json_or(&detail, json, |w| {
        writeln!(
            w,
            "Goal {}  [{}]",
            style(goal.id()).cyan().bold(),
            state_styled(goal.state().as_ref()),
        )?;
        writeln!(w)?;

        writeln!(w, "{}", style("Description").bold())?;
        for line in goal.description().lines() {
            writeln!(w, "  {line}")?;
        }

        writeln!(w)?;
        field(w, "Created", &goal.created_at().to_string())?;
        field(w, "Updated", &goal.updated_at().to_string())?;
        if let Some(completed_at) = goal.completed_at() {
            field(w, "Completed", &completed_at.to_string())?;
        }

        writeln!(w)?;
        writeln!(w, "{}", style("Metrics").bold())?;
        writeln!(
            w,
            "  Tasks: {} total, {} completed, {} failed",
            metrics.task_count(),
            metrics.tasks_completed(),
            metrics.tasks_failed()
        )?;
        writeln!(w, "  Tokens: {}", metrics.total_tokens())?;
        writeln!(w, "  Elapsed: {}ms", metrics.elapsed_ms())?;

        if !tasks.is_empty() {
            writeln!(w)?;
            writeln!(
                w,
                "{:<10} {:<13} {}",
                style("ID").bold().underlined(),
                style("STATE").bold().underlined(),
                style("DESCRIPTION").bold().underlined(),
            )?;
            for task in tasks {
                writeln!(
                    w,
                    "{:<10} {:<13} {}",
                    style(task.id()).cyan(),
                    state_styled(task.state().as_ref()),
                    truncate(task.description(), 80),
                )?;
            }
        }
        Ok(())
    })
}

// -- Ready --

pub fn ready_tasks(tasks: &[Task], goal: &Goal, json: bool) -> Result<()> {
    json_or(tasks, json, |w| {
        writeln!(
            w,
            "Ready tasks for {} [{}]",
            style(goal.id()).cyan().bold(),
            state_styled(goal.state().as_ref()),
        )?;
        writeln!(w)?;

        if tasks.is_empty() {
            writeln!(w, "No tasks ready to start.")?;
            return Ok(());
        }

        writeln!(
            w,
            "{:<10} {}",
            style("ID").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;
        for task in tasks {
            writeln!(
                w,
                "{:<10} {}",
                style(task.id()).cyan(),
                truncate(task.description(), 80),
            )?;
        }
        Ok(())
    })
}

// -- List --

pub fn list(results: &[GoalWithTasks], json: bool) -> Result<()> {
    // For JSON, serialize as an array of goals with nested tasks
    #[derive(Serialize)]
    struct GoalEntry<'a> {
        #[serde(flatten)]
        goal: &'a Goal,
        tasks: &'a [Task],
        metrics: &'a crate::models::Metrics,
    }

    let entries: Vec<GoalEntry> = results
        .iter()
        .map(|r| GoalEntry {
            goal: &r.goal,
            tasks: &r.tasks,
            metrics: &r.metrics,
        })
        .collect();

    json_or(&entries, json, |w| {
        if results.is_empty() {
            writeln!(w, "No goals found.")?;
            return Ok(());
        }

        for r in results {
            let goal = &r.goal;
            let metrics = &r.metrics;

            writeln!(
                w,
                "{}  {}  ({}/{})",
                style(goal.id()).cyan().bold(),
                state_styled(goal.state().as_ref()),
                metrics.tasks_completed(),
                metrics.task_count(),
            )?;
            writeln!(w, "  {}", truncate(goal.description(), 80))?;

            if !r.tasks.is_empty() {
                writeln!(w)?;
                for task in &r.tasks {
                    writeln!(
                        w,
                        "  {:<10} {:<13} {}",
                        style(task.id()).cyan(),
                        state_styled(task.state().as_ref()),
                        truncate(task.description(), 60),
                    )?;
                }
            }
            writeln!(w)?;
        }
        Ok(())
    })
}

// -- Prep --

pub fn prep(text: &str) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(w, "{text}")?;
    Ok(())
}

// -- Helpers --

/// Write a labeled field: `{label}  {value}` with consistent alignment.
fn field(w: &mut dyn Write, label: &str, value: &str) -> Result<()> {
    writeln!(w, "{:<14} {}", style(label).dim(), value)?;
    Ok(())
}

/// Apply color to a state string based on its value.
fn state_styled(state: &str) -> console::StyledObject<&str> {
    match state {
        "completed" => style(state).green(),
        "in_progress" | "verifying" => style(state).yellow(),
        "failed" => style(state).red(),
        "blocked" => style(state).red(),
        "pending" => style(state).dim(),
        _ => style(state).white(),
    }
}
