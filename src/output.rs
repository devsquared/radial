use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::Result;
use console::style;
use serde::Serialize;

use crate::id::TaskId;
use crate::models::{Goal, Task};
use crate::ops::clean::CleanResult;
use crate::ops::compact::CompactCandidate;
use crate::ops::init::InitResult;
use crate::ops::list::GoalWithTasks;
use crate::ops::show::ShowResult;
use crate::ops::status::{GoalSummary, StatusResult};
use crate::ops::task::{CancelResult, CompleteResult};

/// Options that control how output is rendered.
pub struct RenderOptions {
    pub json: bool,
    pub full: bool,
    pub verbose: bool,
    term_width: u16,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOptions {
    pub fn new() -> Self {
        let term_width = console::Term::stdout().size().1;
        Self {
            json: false,
            full: false,
            verbose: false,
            term_width,
        }
    }

    #[must_use]
    pub fn json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    #[must_use]
    pub fn full(mut self, full: bool) -> Self {
        self.full = full;
        self
    }

    #[must_use]
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Max chars available for a description column once `prefix_cols` fixed
    /// columns are accounted for. Returns `usize::MAX` when `--full` is set.
    fn desc_width(&self, prefix_cols: u16) -> usize {
        if self.full {
            usize::MAX
        } else {
            self.term_width.saturating_sub(prefix_cols).max(20) as usize
        }
    }
}

/// Print as JSON if `opts.json` is true, otherwise call `human` with a writer.
fn json_or<T: Serialize + ?Sized>(
    value: &T,
    opts: &RenderOptions,
    human: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if opts.json {
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

pub fn goal_created(goal: &Goal, opts: &RenderOptions) -> Result<()> {
    json_or(goal, opts, |w| {
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

pub fn goal_list(goals: &[Goal], opts: &RenderOptions) -> Result<()> {
    // ID(10) + space(1) + STATE(13) + space(1) = 25 prefix cols
    let desc_w = opts.desc_width(25);
    json_or(goals, opts, |w| {
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
                truncate(goal.description(), desc_w),
            )?;
        }
        Ok(())
    })
}

// -- Edit outputs --

pub fn goal_edited(goal: &Goal) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Updated goal:").green(),
        style(goal.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(goal.description(), 80))?;
    Ok(())
}

pub fn goal_cancelled(goal: &Goal, cancelled_task_ids: &[TaskId]) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Cancelled goal:").dim(),
        style(goal.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(goal.description(), 80))?;

    if !cancelled_task_ids.is_empty() {
        writeln!(w)?;
        writeln!(
            w,
            "{} {}",
            style("Cancelled").dim(),
            style(format!("{} task(s)", cancelled_task_ids.len())).dim()
        )?;
    }

    Ok(())
}

pub fn goal_restored(goal: &Goal) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Restored goal:").green(),
        style(goal.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(goal.description(), 80))?;
    Ok(())
}

// -- Clean outputs --

/// Prompt for confirmation before archiving or deleting a single goal.
pub fn confirm_clean(goal: &Goal, purge: bool) -> Result<bool> {
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

/// Report a single goal having just been archived or deleted.
pub fn clean_removed(goal: &Goal, purge: bool) -> Result<()> {
    let mut w = io::stdout().lock();
    if purge {
        writeln!(
            w,
            "  {} {} — {}",
            style("Deleted").red(),
            style(goal.id()).cyan(),
            truncate(goal.description(), 60),
        )?;
    } else {
        writeln!(
            w,
            "  {} {} — {}",
            style("Archived").dim(),
            style(goal.id()).cyan(),
            truncate(goal.description(), 60),
        )?;
    }
    Ok(())
}

pub fn clean(result: &CleanResult) -> Result<()> {
    let mut w = io::stdout().lock();
    if result.candidates == 0 {
        let msg = if result.force {
            "No goals found."
        } else {
            "No completed or cancelled goals to clean."
        };
        writeln!(w, "{msg}")?;
        return Ok(());
    }

    let action = if result.purge { "Deleted" } else { "Archived" };
    writeln!(
        w,
        "\n{} {} goal(s).",
        action,
        style(result.removed.len()).bold()
    )?;
    Ok(())
}

// -- Init outputs --

pub fn init(result: &InitResult) -> Result<()> {
    let mut w = io::stdout().lock();
    if result.already_initialized {
        writeln!(
            w,
            "Radial already initialized in {}",
            result.radial_dir.display()
        )?;
        return Ok(());
    }

    if let Some(target) = result.gitignore_target {
        writeln!(w, "Added .radial to {}", target.display_path())?;
    }

    writeln!(w, "Initialized radial in {}", result.radial_dir.display())?;
    Ok(())
}

pub fn task_edited(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Updated task:").green(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    Ok(())
}

// -- Task outputs --

pub fn task_created(task: &Task, opts: &RenderOptions) -> Result<()> {
    json_or(task, opts, |w| {
        writeln!(
            w,
            "{} {}",
            style("Created task:").green(),
            style(task.id()).cyan().bold()
        )?;
        writeln!(w, "  {}", truncate(task.description(), 80))?;
        writeln!(w, "  State: {}", state_styled(task.state().as_ref()))?;
        writeln!(w, "  Priority: {}", task.priority().as_ref())?;
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

#[allow(clippy::too_many_lines)]
pub fn task_list(tasks: &[Task], goal: &Goal, opts: &RenderOptions) -> Result<()> {
    // ID(10) + STATE(13) + PRIORITY(10) + ASSIGNEE(12) + 4 spaces = 49 prefix cols
    let desc_w = opts.desc_width(49);
    // Subtasks indent 2 extra cols
    let sub_desc_w = opts.desc_width(51);
    // Comment lines: 11 spaces + timestamp (~20) + 2 spaces = ~33 prefix cols
    let comment_w = opts.desc_width(33);
    json_or(tasks, opts, |w| {
        let goal_ref = goal.display_ref().unwrap_or_else(|| goal.id().to_string());
        writeln!(
            w,
            "Tasks for {} [{}]  {}",
            style(&goal_ref).cyan().bold(),
            state_styled(goal.state().as_ref()),
            style(goal.id()).dim(),
        )?;
        writeln!(w, "  {}", truncate(goal.description(), opts.desc_width(2)))?;
        writeln!(w)?;

        if tasks.is_empty() {
            writeln!(w, "No tasks found.")?;
            return Ok(());
        }

        writeln!(
            w,
            "{:<10} {:<13} {:<10} {:<12} {}",
            style("ID").bold().underlined(),
            style("STATE").bold().underlined(),
            style("PRIORITY").bold().underlined(),
            style("ASSIGNEE").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;

        let subtask_map = build_subtask_map(tasks);
        let goal_seq = goal.seq().unwrap_or(0);

        for task in tasks.iter().filter(|t| t.parent_id().is_none()) {
            let task_ref = task
                .display_ref(goal_seq)
                .unwrap_or_else(|| task.id().to_string());
            writeln!(
                w,
                "{:<10} {:<13} {:<10} {:<12} {}  {}",
                style(&task_ref).cyan(),
                state_styled(task.state().as_ref()),
                task.priority().as_ref(),
                task.assignee().unwrap_or("-"),
                truncate(task.description(), desc_w),
                style(task.id()).dim(),
            )?;
            if opts.verbose && !task.comments().is_empty() {
                let mut truncated = false;
                for comment in task.comments() {
                    let text = comment.text();
                    if text.len() > comment_w {
                        truncated = true;
                    }
                    writeln!(
                        w,
                        "           {}  {}",
                        style(comment.created_at()).dim(),
                        truncate(text, comment_w),
                    )?;
                }
                if truncated {
                    writeln!(
                        w,
                        "           {}",
                        style(format!(
                            "Use 'rd task comments {}' to read full comments",
                            task.id()
                        ))
                        .dim(),
                    )?;
                }
            }
            if let Some(subtasks) = subtask_map.get(task.id()) {
                for subtask in subtasks {
                    let subtask_ref = subtask
                        .display_ref(goal_seq)
                        .unwrap_or_else(|| subtask.id().to_string());
                    writeln!(
                        w,
                        "  {:<8} {:<13} {:<10} {:<12} {}  {}",
                        style(&subtask_ref).cyan(),
                        state_styled(subtask.state().as_ref()),
                        subtask.priority().as_ref(),
                        subtask.assignee().unwrap_or("-"),
                        truncate(subtask.description(), sub_desc_w),
                        style(subtask.id()).dim(),
                    )?;
                    if opts.verbose && !subtask.comments().is_empty() {
                        let mut truncated = false;
                        for comment in subtask.comments() {
                            let text = comment.text();
                            if text.len() > comment_w {
                                truncated = true;
                            }
                            writeln!(
                                w,
                                "             {}  {}",
                                style(comment.created_at()).dim(),
                                truncate(text, comment_w),
                            )?;
                        }
                        if truncated {
                            writeln!(
                                w,
                                "             {}",
                                style(format!(
                                    "Use 'rd task comments {}' to read full comments",
                                    subtask.id()
                                ))
                                .dim(),
                            )?;
                        }
                    }
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
    if let Some(assignee) = task.assignee() {
        writeln!(w, "  Assigned to: {assignee}")?;
    }
    Ok(())
}

pub fn task_released(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Released task:").yellow(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    writeln!(w, "  State: {}", state_styled(task.state().as_ref()))?;
    Ok(())
}

pub fn tasks_released_stale(tasks: &[Task]) -> Result<()> {
    let mut w = io::stdout().lock();
    if tasks.is_empty() {
        writeln!(w, "No stale in-progress tasks found.")?;
        return Ok(());
    }
    writeln!(w, "Released {} stale task(s):", style(tasks.len()).bold())?;
    for task in tasks {
        let assignee = task.assignee().unwrap_or("(none)");
        writeln!(
            w,
            "  {} (assigned to {})",
            style(task.id()).cyan(),
            assignee,
        )?;
    }
    Ok(())
}

pub fn tasks_released_all_in_progress(tasks: &[Task]) -> Result<()> {
    let mut w = io::stdout().lock();
    if tasks.is_empty() {
        writeln!(w, "No in-progress tasks found.")?;
        return Ok(());
    }
    writeln!(
        w,
        "Released {} task(s) from in-progress back to pending.",
        style(tasks.len()).bold()
    )?;
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

pub fn task_cancelled(result: &CancelResult) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Cancelled task:").dim(),
        style(result.task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(result.task.description(), 80))?;

    if !result.unblocked_task_ids.is_empty() {
        writeln!(w)?;
        writeln!(
            w,
            "{} {}",
            style("Unblocked").yellow(),
            style(format!("{} task(s):", result.unblocked_task_ids.len())).yellow()
        )?;
        for id in &result.unblocked_task_ids {
            writeln!(w, "  - {}", style(id).cyan())?;
        }
    }

    if !result.cascaded_task_ids.is_empty() {
        writeln!(w)?;
        writeln!(
            w,
            "{} {}",
            style("Cascade-cancelled").dim(),
            style(format!("{} task(s):", result.cascaded_task_ids.len())).dim()
        )?;
        for id in &result.cascaded_task_ids {
            writeln!(w, "  - {}", style(id).cyan())?;
        }
    }

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

pub fn task_deleted(task: &Task) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Deleted task:").red(),
        style(task.id()).cyan().bold()
    )?;
    writeln!(w, "  {}", truncate(task.description(), 80))?;
    Ok(())
}

pub fn task_comments(task: &Task, opts: &RenderOptions) -> Result<()> {
    json_or(task, opts, |w| {
        writeln!(
            w,
            "Comments for task {} ({} total)",
            style(task.id()).cyan().bold(),
            task.comments().len(),
        )?;
        if task.comments().is_empty() {
            writeln!(w)?;
            writeln!(w, "  {}", style("No comments.").dim())?;
            return Ok(());
        }
        for comment in task.comments() {
            writeln!(w)?;
            writeln!(
                w,
                "  {}",
                style(format!("[{}]", comment.created_at())).dim()
            )?;
            for line in comment.text().lines() {
                writeln!(w, "  {line}")?;
            }
        }
        Ok(())
    })
}

pub fn task_commented(task: &Task, opts: &RenderOptions) -> Result<()> {
    json_or(task, opts, |w| {
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

pub fn status(result: &StatusResult, opts: &RenderOptions) -> Result<()> {
    match result {
        StatusResult::Task(task) => status_task(task, opts),
        StatusResult::Goal(goal_status) => status_goal(goal_status, opts),
        StatusResult::AllGoals(summaries) => status_all_goals(summaries, opts),
    }
}

fn status_task(task: &Task, opts: &RenderOptions) -> Result<()> {
    let desc_w = opts.desc_width(49);
    json_or(task, opts, |w| {
        // Note: We can't show full display ref without goal context,
        // so just show the task ID (or seq if available)
        let task_id_str = task.id().to_string();
        writeln!(
            w,
            "{:<10} {:<13} {:<10} {:<12} {}",
            style(&task_id_str).cyan(),
            state_styled(task.state().as_ref()),
            task.priority().as_ref(),
            task.assignee().unwrap_or("-"),
            truncate(task.description(), desc_w),
        )?;
        Ok(())
    })
}

fn status_goal(goal_status: &crate::ops::status::GoalStatus, opts: &RenderOptions) -> Result<()> {
    let desc_w = opts.desc_width(49);
    json_or(goal_status, opts, |w| {
        let goal = goal_status.goal();
        let metrics = goal_status.metrics();
        let goal_ref = goal.display_ref().unwrap_or_else(|| goal.id().to_string());

        writeln!(
            w,
            "Goal: {}  {}  ({}/{} tasks)  {}",
            style(&goal_ref).cyan().bold(),
            state_styled(goal.state().as_ref()),
            metrics.tasks_completed(),
            metrics.task_count(),
            style(goal.id()).dim(),
        )?;
        writeln!(w, "  {}", truncate(goal.description(), opts.desc_width(2)))?;
        writeln!(w)?;

        if !goal_status.tasks().is_empty() {
            writeln!(
                w,
                "{:<10} {:<13} {:<10} {:<12} {}",
                style("ID").bold().underlined(),
                style("STATE").bold().underlined(),
                style("PRIORITY").bold().underlined(),
                style("ASSIGNEE").bold().underlined(),
                style("DESCRIPTION").bold().underlined(),
            )?;
            let goal_seq = goal.seq().unwrap_or(0);
            for task in goal_status.tasks() {
                let task_ref = task
                    .display_ref(goal_seq)
                    .unwrap_or_else(|| task.id().to_string());
                writeln!(
                    w,
                    "{:<10} {:<13} {:<10} {:<12} {}  {}",
                    style(&task_ref).cyan(),
                    state_styled(task.state().as_ref()),
                    task.priority().as_ref(),
                    task.assignee().unwrap_or("-"),
                    truncate(task.description(), desc_w),
                    style(task.id()).dim(),
                )?;
            }
        }
        Ok(())
    })
}

fn status_all_goals(summaries: &[GoalSummary], opts: &RenderOptions) -> Result<()> {
    // ID(10) + STATE(13) + TASKS(7) + 3 spaces = 33 prefix cols
    let desc_w = opts.desc_width(33);
    json_or(summaries, opts, |w| {
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
            let goal_ref = goal.display_ref().unwrap_or_else(|| goal.id().to_string());
            writeln!(
                w,
                "{:<10} {:<13} {:<7} {}  {}",
                style(&goal_ref).cyan(),
                state_styled(goal.state().as_ref()),
                format!("{}/{}", metrics.tasks_completed(), metrics.task_count()),
                truncate(goal.description(), desc_w),
                style(goal.id()).dim(),
            )?;
        }
        Ok(())
    })
}

// -- Show outputs (full detail) --

pub fn show(result: &ShowResult, opts: &RenderOptions) -> Result<()> {
    match result {
        ShowResult::Task(task) => show_task(task, opts),
        ShowResult::Goal {
            goal,
            tasks,
            metrics,
        } => show_goal(goal, tasks, metrics, opts),
    }
}

fn show_compacted_task(task: &Task, w: &mut dyn Write) -> Result<()> {
    writeln!(w, "{}", style("[compacted]").dim())?;
    if let Some(summary) = task.summary() {
        writeln!(w)?;
        writeln!(w, "{}", style("Summary").bold())?;
        for line in summary.lines() {
            writeln!(w, "  {line}")?;
        }
    }
    if let Some(result) = task.result()
        && !result.artifacts().is_empty()
    {
        writeln!(w)?;
        writeln!(w, "{}", style("Artifacts").bold())?;
        for artifact in result.artifacts() {
            writeln!(w, "  {artifact}")?;
        }
    }
    writeln!(w)?;
    field(w, "Priority", task.priority().as_ref())?;
    field(w, "Goal", task.goal_id().as_ref())?;
    field(w, "Created", &task.created_at().to_string())?;
    field(w, "Updated", &task.updated_at().to_string())?;
    Ok(())
}

fn show_task(task: &Task, opts: &RenderOptions) -> Result<()> {
    json_or(task, opts, |w| {
        // Note: We can't show full display ref without goal context
        writeln!(
            w,
            "Task {}  [{}]",
            style(task.id()).cyan().bold(),
            state_styled(task.state().as_ref()),
        )?;
        writeln!(w)?;

        if task.compacted() {
            return show_compacted_task(task, w);
        }

        writeln!(w, "{}", style("Description").bold())?;
        for line in task.description().lines() {
            writeln!(w, "  {line}")?;
        }

        writeln!(w)?;
        field(w, "Priority", task.priority().as_ref())?;
        field(w, "Goal", task.goal_id().as_ref())?;
        if let Some(parent_id) = task.parent_id() {
            field(w, "Parent", parent_id.as_ref())?;
        }
        if let Some(assignee) = task.assignee() {
            field(w, "Assignee", assignee)?;
        }
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
            let ids: Vec<&str> = task.blocked_by().iter().map(AsRef::as_ref).collect();
            field(w, "Blocked by", &ids.join(", "))?;
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
                writeln!(
                    w,
                    "  {}",
                    style(format!("[{}]", comment.created_at())).dim()
                )?;
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
    opts: &RenderOptions,
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

    let desc_w = opts.desc_width(49);
    json_or(&detail, opts, |w| {
        let goal_ref = goal.display_ref().unwrap_or_else(|| goal.id().to_string());
        writeln!(
            w,
            "Goal {}  [{}]  {}",
            style(&goal_ref).cyan().bold(),
            state_styled(goal.state().as_ref()),
            style(goal.id()).dim(),
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
        write!(
            w,
            "  Tasks: {} total, {} completed",
            metrics.task_count(),
            metrics.tasks_completed()
        )?;
        if metrics.tasks_cancelled() > 0 {
            write!(w, ", {} cancelled", metrics.tasks_cancelled())?;
        }
        if metrics.tasks_failed() > 0 {
            write!(w, ", {} failed", metrics.tasks_failed())?;
        }
        writeln!(w)?;
        writeln!(w, "  Tokens: {}", metrics.total_tokens())?;
        writeln!(w, "  Elapsed: {}ms", metrics.elapsed_ms())?;

        if !tasks.is_empty() {
            writeln!(w)?;
            writeln!(
                w,
                "{:<10} {:<13} {:<10} {:<12} {}",
                style("ID").bold().underlined(),
                style("STATE").bold().underlined(),
                style("PRIORITY").bold().underlined(),
                style("ASSIGNEE").bold().underlined(),
                style("DESCRIPTION").bold().underlined(),
            )?;
            for task in tasks {
                writeln!(
                    w,
                    "{:<10} {:<13} {:<10} {:<12} {}",
                    style(task.id()).cyan(),
                    state_styled(task.state().as_ref()),
                    task.priority().as_ref(),
                    task.assignee().unwrap_or("-"),
                    truncate(task.description(), desc_w),
                )?;
            }
        }
        Ok(())
    })
}

// -- Ready --

pub fn ready_tasks(
    tasks: &[(Task, Option<Task>)],
    goal: &Goal,
    stale_count: usize,
    opts: &RenderOptions,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct ReadyTaskJson<'a> {
        #[serde(flatten)]
        task: &'a Task,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent: Option<&'a Task>,
    }

    let task_refs: Vec<ReadyTaskJson> = tasks
        .iter()
        .map(|(t, p)| ReadyTaskJson {
            task: t,
            parent: p.as_ref(),
        })
        .collect();

    json_or(&task_refs, opts, |w| {
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

        for (task, parent) in tasks {
            writeln!(
                w,
                "{} [{}]",
                style(task.id()).cyan().bold(),
                task.priority().as_ref(),
            )?;
            writeln!(w, "  {}", task.description())?;
            if let Some(p) = parent {
                writeln!(
                    w,
                    "  {} {} — {}",
                    style("Parent:").dim(),
                    style(p.id()).cyan(),
                    truncate(p.description(), 60),
                )?;
            }
            if let Some(contract) = task.contract() {
                writeln!(w, "  Contract:")?;
                writeln!(w, "    Receives: {}", contract.receives())?;
                writeln!(w, "    Produces: {}", contract.produces())?;
                writeln!(w, "    Verify:   {}", contract.verify())?;
            }
            writeln!(w)?;
        }

        if stale_count > 0 {
            writeln!(
                w,
                "### Stale Tasks\n\n\
                 There are {stale_count} task(s) that have been in progress for over 2 hours.\n\
                 Run `rd task release --stale 2h` to release them."
            )?;
        }

        Ok(())
    })
}

// -- List --

pub fn list(results: &[GoalWithTasks], opts: &RenderOptions) -> Result<()> {
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

    // 2 indent + ID(10) + STATE(13) + PRIORITY(10) + ASSIGNEE(12) + 4 spaces = 51 prefix cols
    let task_desc_w = opts.desc_width(51);
    // Subtasks add 2 more indent cols
    let sub_desc_w = opts.desc_width(53);
    let goal_desc_w = opts.desc_width(2);
    json_or(&entries, opts, |w| {
        if results.is_empty() {
            writeln!(w, "No goals found.")?;
            return Ok(());
        }

        for r in results {
            let goal = &r.goal;
            let metrics = &r.metrics;

            let goal_ref = goal.display_ref().unwrap_or_else(|| goal.id().to_string());
            writeln!(
                w,
                "{}  {}  ({}/{})  {}",
                style(&goal_ref).cyan().bold(),
                truncate(goal.description(), goal_desc_w),
                metrics.tasks_completed(),
                metrics.task_count(),
                style(goal.id()).dim(),
            )?;

            if !r.tasks.is_empty() {
                writeln!(w)?;
                let subtask_map = build_subtask_map(&r.tasks);
                let goal_seq = goal.seq().unwrap_or(0);
                for task in r.tasks.iter().filter(|t| t.parent_id().is_none()) {
                    let task_ref = task
                        .display_ref(goal_seq)
                        .unwrap_or_else(|| task.id().to_string());
                    writeln!(
                        w,
                        "  {:<10} {:<13} {:<10} {:<12} {}  {}",
                        style(&task_ref).cyan(),
                        state_styled(task.state().as_ref()),
                        task.priority().as_ref(),
                        task.assignee().unwrap_or("-"),
                        truncate(task.description(), task_desc_w),
                        style(task.id()).dim(),
                    )?;
                    if let Some(subtasks) = subtask_map.get(task.id()) {
                        for subtask in subtasks {
                            let subtask_ref = subtask
                                .display_ref(goal_seq)
                                .unwrap_or_else(|| subtask.id().to_string());
                            writeln!(
                                w,
                                "    {:<8} {:<13} {:<10} {:<12} {}  {}",
                                style(&subtask_ref).cyan(),
                                state_styled(subtask.state().as_ref()),
                                subtask.priority().as_ref(),
                                subtask.assignee().unwrap_or("-"),
                                truncate(subtask.description(), sub_desc_w),
                                style(subtask.id()).dim(),
                            )?;
                        }
                    }
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

// -- Compact --

pub fn compact_analyze(candidates: &[CompactCandidate], opts: &RenderOptions) -> Result<()> {
    let desc_w = opts.desc_width(35);
    json_or(candidates, opts, |w| {
        if candidates.is_empty() {
            writeln!(w, "No tasks eligible for compaction.")?;
            return Ok(());
        }

        writeln!(w, "{} task(s) eligible for compaction:\n", candidates.len())?;
        writeln!(
            w,
            "{:<10} {:<10} {:<13} {}",
            style("ID").bold().underlined(),
            style("GOAL").bold().underlined(),
            style("STATE").bold().underlined(),
            style("DESCRIPTION").bold().underlined(),
        )?;
        for c in candidates {
            writeln!(
                w,
                "{:<10} {:<10} {:<13} {}",
                style(&c.id).cyan(),
                style(&c.goal_id).dim(),
                state_styled(&c.state),
                truncate(&c.description, desc_w),
            )?;
        }
        Ok(())
    })
}

pub fn compact_apply(task_id: &str) -> Result<()> {
    let mut w = io::stdout().lock();
    writeln!(
        w,
        "{} {}",
        style("Compacted task:").green(),
        style(task_id).cyan().bold()
    )?;
    Ok(())
}

// -- Helpers --

/// Write a labeled field: `{label}  {value}` with consistent alignment.
fn field(w: &mut dyn Write, label: &str, value: &str) -> Result<()> {
    writeln!(w, "{:<14} {}", style(label).dim(), value)?;
    Ok(())
}

/// Group subtasks by their parent task ID for hierarchical rendering.
fn build_subtask_map(tasks: &[Task]) -> HashMap<&TaskId, Vec<&Task>> {
    let mut map: HashMap<&TaskId, Vec<&Task>> = HashMap::new();
    for task in tasks {
        if let Some(pid) = task.parent_id() {
            map.entry(pid).or_default().push(task);
        }
    }
    map
}

/// Apply color to a state string based on its value.
fn state_styled(state: &str) -> console::StyledObject<&str> {
    match state {
        "completed" => style(state).green(),
        "in_progress" | "verifying" => style(state).yellow(),
        "failed" | "blocked" => style(state).red(),
        "pending" => style(state).dim(),
        _ => style(state).white(),
    }
}
