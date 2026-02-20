use anyhow::Result;
use console::{style, Term};
use serde::Serialize;

use crate::commands::status::{GoalStatus, GoalSummary, StatusResult};
use crate::commands::task::CompleteResult;
use crate::models::{Goal, Task};

pub struct Output {
    term: Term,
    json: bool,
    concise: bool,
    verbose: bool,
}

impl Output {
    pub fn new(json: bool) -> Self {
        Self {
            term: Term::stdout(),
            json,
            concise: false,
            verbose: false,
        }
    }

    pub fn with_concise(json: bool, concise: bool) -> Self {
        Self {
            term: Term::stdout(),
            json,
            concise,
            verbose: false,
        }
    }

    pub fn with_verbose(json: bool, verbose: bool) -> Self {
        Self {
            term: Term::stdout(),
            json,
            concise: false,
            verbose,
        }
    }

    fn print_json<T: Serialize + ?Sized>(&self, value: &T) -> Result<()> {
        let output = serde_json::to_string_pretty(value)?;
        self.term.write_line(&output)?;
        Ok(())
    }

    pub fn goal_created(&self, goal: &Goal) -> Result<()> {
        if self.json {
            return self.print_json(goal);
        }

        self.term.write_line(&format!(
            "{} {}",
            style("Created goal:").green(),
            style(&goal.id).cyan().bold()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", goal.description))?;
        Ok(())
    }

    pub fn goal_list(&self, goals: &[Goal]) -> Result<()> {
        if self.json {
            return self.print_json(goals);
        }

        if goals.is_empty() {
            self.term.write_line("No goals found.")?;
            return Ok(());
        }

        for goal in goals {
            self.term.write_line(&format!(
                "{} [{}]",
                style(&goal.id).cyan().bold(),
                style(goal.state.as_ref()).yellow()
            ))?;
            self.term
                .write_line(&format!("  Description: {}", goal.description))?;
            self.term.write_line("")?;
        }
        Ok(())
    }

    pub fn task_created(&self, task: &Task) -> Result<()> {
        if self.json {
            return self.print_json(task);
        }

        self.term.write_line(&format!(
            "{} {}",
            style("Created task:").green(),
            style(&task.id).cyan().bold()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;
        self.term
            .write_line(&format!("  State: {}", style(task.state.as_ref()).yellow()))?;
        if task.contract.is_none() {
            self.term.write_line(&format!(
                "  Contract: {}",
                style("(not set - required before starting)").dim()
            ))?;
        }
        Ok(())
    }

    pub fn task_list(&self, tasks: &[Task], goal: &Goal) -> Result<()> {
        if self.json {
            return self.print_json(tasks);
        }

        self.term.write_line(&format!(
            "Tasks for goal: {} [{}]",
            style(&goal.id).cyan().bold(),
            style(goal.state.as_ref()).yellow()
        ))?;
        self.term.write_line(&format!("  {}", goal.description))?;
        self.term.write_line("")?;

        if tasks.is_empty() {
            self.term.write_line("No tasks found.")?;
            return Ok(());
        }

        for task in tasks {
            self.print_task_summary(task)?;
            if self.verbose && !task.comments.is_empty() {
                self.term
                    .write_line(&format!("  Comments: ({})", task.comments.len()))?;
                for comment in &task.comments {
                    self.term.write_line(&format!(
                        "    [{}] {}",
                        style(&comment.created_at).dim(),
                        comment.text
                    ))?;
                }
            }
            self.term.write_line("")?;
        }
        Ok(())
    }

    fn print_task_summary(&self, task: &Task) -> Result<()> {
        self.term.write_line(&format!(
            "{} [{}]",
            style(&task.id).cyan().bold(),
            style(task.state.as_ref()).yellow()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;

        if let Some(ref contract) = task.contract {
            self.term.write_line("  Contract:")?;
            self.term
                .write_line(&format!("    Receives: {}", contract.receives))?;
            self.term
                .write_line(&format!("    Produces: {}", contract.produces))?;
            self.term
                .write_line(&format!("    Verify: {}", contract.verify))?;
        } else {
            self.term
                .write_line(&format!("  Contract: {}", style("(not set)").dim()))?;
        }

        if !task.blocked_by.is_empty() {
            self.term
                .write_line(&format!("  Blocked by: {}", task.blocked_by.join(", ")))?;
        }

        if let Some(result) = &task.result {
            self.term
                .write_line(&format!("  Result: {}", result.summary))?;
            if !result.artifacts.is_empty() {
                self.term
                    .write_line(&format!("  Artifacts: {}", result.artifacts.join(", ")))?;
            }
        }
        Ok(())
    }

    pub fn task_started(&self, task: &Task) -> Result<()> {
        self.term.write_line(&format!(
            "{} {}",
            style("Started task:").green(),
            style(&task.id).cyan().bold()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;
        Ok(())
    }

    pub fn task_completed(&self, result: &CompleteResult) -> Result<()> {
        self.term.write_line(&format!(
            "{} {}",
            style("Completed task:").green(),
            style(&result.task.id).cyan().bold()
        ))?;
        if let Some(ref res) = result.task.result {
            self.term
                .write_line(&format!("  Result: {}", res.summary))?;
        }

        if !result.unblocked_task_ids.is_empty() {
            self.term.write_line("")?;
            self.term
                .write_line(&style("Unblocked tasks:").yellow().to_string())?;
            for id in &result.unblocked_task_ids {
                self.term.write_line(&format!("  - {}", style(id).cyan()))?;
            }
        }
        Ok(())
    }

    pub fn task_failed(&self, task: &Task) -> Result<()> {
        self.term.write_line(&format!(
            "{} {}",
            style("Failed task:").red(),
            style(&task.id).cyan().bold()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;
        Ok(())
    }

    pub fn task_retry(&self, task: &Task) -> Result<()> {
        self.term.write_line(&format!(
            "{} {}",
            style("Retrying task:").yellow(),
            style(&task.id).cyan().bold()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;
        self.term
            .write_line(&format!("  Retry count: {}", task.metrics.retry_count))?;
        Ok(())
    }

    pub fn task_commented(&self, task: &Task) -> Result<()> {
        if self.json {
            return self.print_json(task);
        }

        self.term.write_line(&format!(
            "{} {}",
            style("Added comment to task:").green(),
            style(&task.id).cyan().bold()
        ))?;
        if let Some(comment) = task.comments.last() {
            self.term
                .write_line(&format!("  Comment: {}", comment.text))?;
        }
        self.term
            .write_line(&format!("  Total comments: {}", task.comments.len()))?;
        Ok(())
    }

    pub fn status(&self, result: &StatusResult) -> Result<()> {
        match result {
            StatusResult::Task(task) => self.status_task(task),
            StatusResult::Goal(goal_status) => self.status_goal(goal_status),
            StatusResult::AllGoals(summaries) => self.status_all_goals(summaries),
        }
    }

    fn status_task(&self, task: &Task) -> Result<()> {
        if self.json {
            return self.print_json(task);
        }

        self.term.write_line(&format!(
            "Task: {} [{}]",
            style(&task.id).cyan().bold(),
            style(task.state.as_ref()).yellow()
        ))?;
        self.term.write_line(&format!("  Goal: {}", task.goal_id))?;
        self.term
            .write_line(&format!("  Description: {}", task.description))?;
        self.term
            .write_line(&format!("  Created: {}", task.created_at))?;
        self.term
            .write_line(&format!("  Updated: {}", task.updated_at))?;
        self.term.write_line("")?;

        if let Some(ref contract) = task.contract {
            self.term
                .write_line(&style("Contract:").bold().to_string())?;
            self.term
                .write_line(&format!("  Receives: {}", contract.receives))?;
            self.term
                .write_line(&format!("  Produces: {}", contract.produces))?;
            self.term
                .write_line(&format!("  Verify: {}", contract.verify))?;
        } else {
            self.term
                .write_line(&format!("Contract: {}", style("(not set)").dim()))?;
        }

        if !task.blocked_by.is_empty() {
            self.term.write_line("")?;
            self.term
                .write_line(&format!("Blocked by: {}", task.blocked_by.join(", ")))?;
        }

        if let Some(result) = &task.result {
            self.term.write_line("")?;
            self.term.write_line(&style("Result:").bold().to_string())?;
            self.term
                .write_line(&format!("  Summary: {}", result.summary))?;
            if !result.artifacts.is_empty() {
                self.term.write_line("  Artifacts:")?;
                for artifact in &result.artifacts {
                    self.term.write_line(&format!("    - {artifact}"))?;
                }
            }
        }

        self.term.write_line("")?;
        self.term
            .write_line(&style("Metrics:").bold().to_string())?;
        self.term
            .write_line(&format!("  Tokens: {}", task.metrics.tokens))?;
        self.term
            .write_line(&format!("  Elapsed: {}ms", task.metrics.elapsed_ms))?;
        self.term
            .write_line(&format!("  Retries: {}", task.metrics.retry_count))?;

        if !self.concise && !task.comments.is_empty() {
            self.term.write_line("")?;
            self.term
                .write_line(&style("Comments:").bold().to_string())?;
            for comment in &task.comments {
                self.term.write_line(&format!(
                    "  [{}] {}",
                    style(&comment.created_at).dim(),
                    comment.text
                ))?;
            }
        }

        Ok(())
    }

    fn status_goal(&self, goal_status: &GoalStatus) -> Result<()> {
        if self.json {
            return self.print_json(goal_status);
        }

        let goal = &goal_status.goal;
        let metrics = &goal_status.metrics;

        self.term.write_line(&format!(
            "Goal: {} [{}]",
            style(&goal.id).cyan().bold(),
            style(goal.state.as_ref()).yellow()
        ))?;
        self.term
            .write_line(&format!("  Description: {}", goal.description))?;
        self.term
            .write_line(&format!("  Created: {}", goal.created_at))?;
        self.term
            .write_line(&format!("  Updated: {}", goal.updated_at))?;
        if let Some(completed_at) = &goal.completed_at {
            self.term
                .write_line(&format!("  Completed: {completed_at}"))?;
        }
        self.term.write_line("")?;

        self.term
            .write_line(&style("Metrics:").bold().to_string())?;
        self.term.write_line(&format!(
            "  Tasks: {} total, {} completed, {} failed",
            metrics.task_count, metrics.tasks_completed, metrics.tasks_failed
        ))?;
        self.term
            .write_line(&format!("  Tokens: {}", metrics.total_tokens))?;
        self.term
            .write_line(&format!("  Elapsed: {}ms", metrics.elapsed_ms))?;

        if !goal_status.tasks.is_empty() {
            self.term.write_line("")?;
            self.term.write_line(&style("Tasks:").bold().to_string())?;
            for task in &goal_status.tasks {
                self.term.write_line(&format!(
                    "  {} [{}] - {}",
                    style(&task.id).cyan(),
                    style(task.state.as_ref()).yellow(),
                    task.description
                ))?;
            }
        }
        Ok(())
    }

    fn status_all_goals(&self, summaries: &[GoalSummary]) -> Result<()> {
        if self.json {
            return self.print_json(summaries);
        }

        if summaries.is_empty() {
            self.term.write_line("No goals found.")?;
            return Ok(());
        }

        self.term
            .write_line(&style("All Goals:").bold().to_string())?;
        self.term.write_line("")?;

        for summary in summaries {
            let goal = &summary.goal;
            let metrics = &summary.computed_metrics;

            self.term.write_line(&format!(
                "{} [{}]",
                style(&goal.id).cyan().bold(),
                style(goal.state.as_ref()).yellow()
            ))?;
            self.term
                .write_line(&format!("  Description: {}", goal.description))?;
            self.term.write_line(&format!(
                "  Tasks: {} total, {} completed, {} failed",
                metrics.task_count, metrics.tasks_completed, metrics.tasks_failed
            ))?;
            self.term.write_line("")?;
        }
        Ok(())
    }

    pub fn ready_tasks(&self, tasks: &[Task], goal: &Goal) -> Result<()> {
        if self.json {
            return self.print_json(tasks);
        }

        self.term.write_line(&format!(
            "Ready tasks for goal: {} [{}]",
            style(&goal.id).cyan().bold(),
            style(goal.state.as_ref()).yellow()
        ))?;
        self.term.write_line(&format!("  {}", goal.description))?;
        self.term.write_line("")?;

        if tasks.is_empty() {
            self.term.write_line("No tasks ready to start.")?;
            return Ok(());
        }

        self.term.write_line(&format!(
            "{} task(s) ready:",
            style(tasks.len()).green().bold()
        ))?;
        self.term.write_line("")?;

        for task in tasks {
            self.term
                .write_line(&style(&task.id).cyan().bold().to_string())?;
            self.term
                .write_line(&format!("  Description: {}", task.description))?;
            if let Some(ref contract) = task.contract {
                self.term
                    .write_line(&format!("  Receives: {}", contract.receives))?;
                self.term
                    .write_line(&format!("  Produces: {}", contract.produces))?;
                self.term
                    .write_line(&format!("  Verify: {}", contract.verify))?;
            }
            self.term.write_line("")?;
        }
        Ok(())
    }

    pub fn prep(&self, text: &str) -> Result<()> {
        self.term.write_line(text)?;
        Ok(())
    }
}
