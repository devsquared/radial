//! `rd mcp`: expose `radial_core`'s `ops` as an MCP (Model Context Protocol)
//! stdio server. Every tool here is a thin wrapper around a single `ops`
//! call -- no business logic lives here. The one exception is
//! `task_complete`/`task_verify`, which share the lock-release-around-a-
//! child-process dance with the CLI via `crate::{complete_task_with_verify,
//! verify_task_dry_run}` rather than duplicating it.
//!
//! Each tool call opens its own `Database` handle (under the process's
//! current working directory, exactly like the CLI) and drops it before
//! returning -- no state is held across calls, matching the "stateless
//! reload-per-call under the existing lock" model the rest of radial uses.

// Every #[tool] method takes &self because the rmcp trait machinery
// requires an instance method, even though none of these bodies touch
// self (there's no server-side state -- see the module doc comment above).
#![allow(clippy::unused_self)]

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::transport::stdio;
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use radial_core::{CoreError, Database, GoalId, Priority, Task, TaskId, ops};

use crate::{
    complete_task_with_verify, ensure_initialized_for_read, ensure_initialized_for_write,
    resolve_goal_or_default, verify_task_dry_run,
};

/// Converts an `ops`-layer failure into an MCP tool error. Attaches the
/// `radial_core::CoreError` code as structured `data` when the failure has
/// one, so a caller can match on it instead of parsing the message -- the
/// same contract `error.rs` documents for any downcasting consumer.
fn to_mcp_error(err: &anyhow::Error) -> McpError {
    let data = err
        .downcast_ref::<CoreError>()
        .map(|e| serde_json::json!({ "code": e.code() }));
    McpError::invalid_params(err.to_string(), data)
}

fn ok_value(value: &serde_json::Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn ok_json<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let value =
        serde_json::to_value(value).map_err(|e| McpError::internal_error(e.to_string(), None))?;
    ok_value(&value)
}

fn resolve_goal(db: &Database, id: &str) -> Result<GoalId, McpError> {
    db.resolve_any_goal(id)
        .map_err(|e| McpError::invalid_params(e.to_string(), None))
}

fn resolve_task(db: &Database, id: &str) -> Result<TaskId, McpError> {
    db.resolve_any_task(id)
        .map_err(|e| McpError::invalid_params(e.to_string(), None))
}

fn resolve_task_ids(db: &Database, ids: &[String]) -> Result<Vec<TaskId>, McpError> {
    ids.iter().map(|id| resolve_task(db, id)).collect()
}

fn resolve_opt_task_ids(
    db: &Database,
    ids: Option<Vec<String>>,
) -> Result<Option<Vec<TaskId>>, McpError> {
    ids.map(|v| resolve_task_ids(db, &v)).transpose()
}

fn parse_priority(raw: Option<String>) -> Result<Option<Priority>, McpError> {
    raw.map(|s| {
        Priority::from_str(&s).map_err(|_| {
            McpError::invalid_params(
                format!("invalid priority '{s}': expected p0, p1, p2, or p3"),
                None,
            )
        })
    })
    .transpose()
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GoalCreateParams {
    /// The goal's description.
    description: String,
    /// Tags to set on the goal.
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GoalCancelParams {
    /// The goal's ID (full ID or an unambiguous prefix).
    goal_id: String,
    /// Why the goal is being cancelled.
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GoalEditParams {
    /// The goal's ID (full ID or an unambiguous prefix).
    goal_id: String,
    /// The goal's new description.
    description: String,
    /// Tags to upsert (existing keys are overwritten; other keys are left alone).
    #[serde(default)]
    tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetTagsParams {
    /// The goal or task ID (full ID or an unambiguous prefix).
    id: String,
    /// Tags to upsert (existing keys are overwritten; other keys are left alone).
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IdParams {
    /// A goal or task ID (full ID or an unambiguous prefix).
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskIdParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskForceParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Act on this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
}

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Deserialize, JsonSchema)]
struct TaskCreateParams {
    /// The task's parent goal ID (full ID or an unambiguous prefix).
    goal_id: String,
    /// The task's description.
    description: String,
    /// Priority: p0 (critical), p1 (high), p2 (normal, default), p3 (low).
    #[serde(default)]
    priority: Option<String>,
    /// Parent task ID, to create this as a subtask.
    #[serde(default)]
    parent_id: Option<String>,
    /// What this task receives (contract).
    #[serde(default)]
    receives: Option<String>,
    /// What this task produces (contract).
    #[serde(default)]
    produces: Option<String>,
    /// How to verify success, in words (contract).
    #[serde(default)]
    verify: Option<String>,
    /// Shell command that checks `verify`; must exit 0. Run by `task_verify`
    /// and gates `task_complete` (see `skip_verify` there).
    #[serde(default)]
    verify_cmd: Option<String>,
    /// IDs of tasks this task is blocked by.
    #[serde(default)]
    blocked_by: Option<Vec<String>>,
    /// Tags to set on the task.
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskListParams {
    /// The goal ID to list tasks for. If omitted, uses the single active
    /// goal, if there is exactly one.
    #[serde(default)]
    goal_id: Option<String>,
    /// Filter by priority level (p0, p1, p2, p3).
    #[serde(default)]
    priority: Option<String>,
    /// Filter by assignee.
    #[serde(default)]
    assignee: Option<String>,
    /// Only include tasks matching every one of these tags.
    #[serde(default)]
    tag_filter: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskStartParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Who is claiming this task.
    assignee: String,
    /// Override blocked state and start anyway.
    #[serde(default)]
    force: bool,
}

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Deserialize, JsonSchema)]
struct TaskCompleteParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Summary of what was accomplished.
    result: String,
    /// Artifact paths created.
    #[serde(default)]
    artifacts: Option<Vec<String>>,
    /// Total tokens used for this task.
    #[serde(default)]
    tokens: Option<i64>,
    /// Elapsed time in milliseconds. Derived from the task's start time if omitted.
    #[serde(default)]
    elapsed: Option<i64>,
    /// Complete this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
    /// Skip the contract's `verify_cmd` for this completion only (recorded
    /// as a comment). Has no effect if no `verify_cmd` is set.
    #[serde(default)]
    skip_verify: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskFailParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Reason for failure.
    #[serde(default)]
    reason: Option<String>,
    /// Compact the task immediately using the reason as its summary. Requires `reason`.
    #[serde(default)]
    compact: bool,
    /// Act on this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskCancelParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Reason for cancellation.
    #[serde(default)]
    reason: Option<String>,
    /// Cascade cancellation to all downstream dependent tasks.
    #[serde(default)]
    cascade: bool,
    /// Act on this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
}

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Deserialize, JsonSchema)]
struct TaskAcceptParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Summary of what was accomplished.
    result: String,
    /// Artifact paths created.
    #[serde(default)]
    artifacts: Option<Vec<String>>,
    /// Total tokens used for this task.
    #[serde(default)]
    tokens: Option<i64>,
    /// Elapsed time in milliseconds. Derived from the task's start time if omitted.
    #[serde(default)]
    elapsed: Option<i64>,
    /// Act on this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskRejectParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// Reason the task was rejected.
    reason: String,
    /// Act on this task directly even though it has open subtasks.
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskReleaseStaleParams {
    /// Release tasks in progress longer than this duration (e.g. "1h", "30m", "2h30m").
    duration: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskCommentParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// The comment text.
    text: String,
}

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Deserialize, JsonSchema)]
struct TaskEditParams {
    /// The task's ID (full ID or an unambiguous prefix).
    task_id: String,
    /// New description.
    #[serde(default)]
    description: Option<String>,
    /// New priority level (p0, p1, p2, p3).
    #[serde(default)]
    priority: Option<String>,
    /// New receives (contract).
    #[serde(default)]
    receives: Option<String>,
    /// New produces (contract).
    #[serde(default)]
    produces: Option<String>,
    /// New verify (contract).
    #[serde(default)]
    verify: Option<String>,
    /// New `verify_cmd` (contract).
    #[serde(default)]
    verify_cmd: Option<String>,
    /// Remove the contract's `verify_cmd`.
    #[serde(default)]
    clear_verify_cmd: bool,
    /// Add dependencies to the task's existing blocked-by list.
    #[serde(default)]
    add_blocked_by: Option<Vec<String>>,
    /// Remove dependencies from the task's existing blocked-by list.
    #[serde(default)]
    remove_blocked_by: Option<Vec<String>>,
    /// Replace the task's entire blocked-by list.
    #[serde(default)]
    set_blocked_by: Option<Vec<String>>,
    /// Tags to upsert (existing keys are overwritten; other keys are left alone).
    #[serde(default)]
    tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadyParams {
    /// The goal ID to check. If omitted, uses the single active goal, if
    /// there is exactly one.
    #[serde(default)]
    goal_id: Option<String>,
    /// Filter by priority level (p0, p1, p2, p3).
    #[serde(default)]
    priority: Option<String>,
    /// Only include tasks matching every one of these tags.
    #[serde(default)]
    tag_filter: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct StatusParams {
    /// Show status of a specific goal.
    #[serde(default)]
    goal_id: Option<String>,
    /// Show status of a specific task.
    #[serde(default)]
    task_id: Option<String>,
    /// Filter tasks by assignee.
    #[serde(default)]
    assignee: Option<String>,
    /// Only include tasks matching every one of these tags.
    #[serde(default)]
    tag_filter: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetMyWorkParams {
    /// The goal ID to pick from. If omitted, uses the single active goal, if
    /// there is exactly one.
    #[serde(default)]
    goal_id: Option<String>,
    /// Only consider tasks at this priority.
    #[serde(default)]
    priority: Option<String>,
    /// Only consider tasks matching every one of these tags.
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

/// MCP server exposing radial's operations as tools. Holds no database
/// state -- see the module doc comment.
#[derive(Clone)]
pub(crate) struct RadialMcp {
    tool_router: ToolRouter<RadialMcp>,
}

#[tool_router]
impl RadialMcp {
    /// Builds the server, disabling every tool not named in `allowed_tools`
    /// (when given). `None` means every tool stays enabled -- the host
    /// (e.g. an orchestrator) opts into narrowing the set, radial has no
    /// notion of roles of its own.
    fn new(allowed_tools: Option<&HashSet<String>>) -> Self {
        let mut tool_router = Self::tool_router();
        if let Some(allowed) = allowed_tools {
            let all_names: Vec<String> = tool_router
                .list_all()
                .into_iter()
                .map(|t| t.name.to_string())
                .collect();
            for name in all_names {
                if !allowed.contains(&name) {
                    tool_router.disable_route(name);
                }
            }
        }
        Self { tool_router }
    }

    #[tool(description = "Create a new goal")]
    fn goal_create(
        &self,
        Parameters(GoalCreateParams { description, tags }): Parameters<GoalCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let goal = ops::goal::create(description, tags, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&goal)
    }

    #[tool(description = "List every non-archived goal")]
    fn goal_list(&self) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        ok_json(&ops::goal::list(&db))
    }

    #[tool(description = "Cancel a goal and all its non-terminal tasks")]
    fn goal_cancel(
        &self,
        Parameters(GoalCancelParams { goal_id, reason }): Parameters<GoalCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let goal_id = resolve_goal(&db, &goal_id)?;
        let result =
            ops::goal::cancel(&goal_id, reason, "mcp", &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&result)
    }

    #[tool(description = "Update a goal's description and/or tags")]
    fn goal_edit(
        &self,
        Parameters(GoalEditParams {
            goal_id,
            description,
            tags,
        }): Parameters<GoalEditParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let goal_id = resolve_goal(&db, &goal_id)?;
        let goal =
            ops::edit::goal(&goal_id, description, tags, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&goal)
    }

    #[tool(description = "Upsert tags onto a goal")]
    fn goal_set_tags(
        &self,
        Parameters(SetTagsParams { id, tags }): Parameters<SetTagsParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let goal_id = resolve_goal(&db, &id)?;
        let goal = ops::goal::set_tags(&goal_id, tags, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&goal)
    }

    #[tool(
        description = "Show full detail of a goal or task by ID (tries task resolution first, then goal)"
    )]
    fn show(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let result = ops::show::run(&id, &db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&result)
    }

    #[tool(description = "Create a new task under a goal")]
    fn task_create(
        &self,
        Parameters(TaskCreateParams {
            goal_id,
            description,
            priority,
            parent_id,
            receives,
            produces,
            verify,
            verify_cmd,
            blocked_by,
            tags,
        }): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let goal_id = resolve_goal(&db, &goal_id)?;
        let parent_id = parent_id.map(|p| resolve_task(&db, &p)).transpose()?;
        let priority = parse_priority(priority)?.unwrap_or_default();
        let blocked_by = resolve_opt_task_ids(&db, blocked_by)?;
        let task = ops::task::create(
            &goal_id,
            description,
            priority,
            parent_id,
            receives,
            produces,
            verify,
            verify_cmd,
            blocked_by,
            tags,
            &mut db,
        )
        .map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "List tasks for a goal")]
    fn task_list(
        &self,
        Parameters(TaskListParams {
            goal_id,
            priority,
            assignee,
            tag_filter,
        }): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let goal_id =
            resolve_goal_or_default(goal_id.as_deref(), &db).map_err(|e| to_mcp_error(&e))?;
        let priority = parse_priority(priority)?;
        let tasks = ops::task::list(
            &goal_id,
            priority.as_ref(),
            assignee.as_deref(),
            &tag_filter,
            &db,
        )
        .map_err(|e| to_mcp_error(&e))?;
        ok_json(&tasks)
    }

    #[tool(description = "Upsert tags onto a task")]
    fn task_set_tags(
        &self,
        Parameters(SetTagsParams { id, tags }): Parameters<SetTagsParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &id)?;
        let task = ops::task::set_tags(&task_id, tags, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Claim a task: assign it and transition it to in_progress")]
    fn task_start(
        &self,
        Parameters(TaskStartParams {
            task_id,
            assignee,
            force,
        }): Parameters<TaskStartParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task =
            ops::task::start(&task_id, &assignee, force, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(
        description = "Complete an in_progress task. Runs the contract's verify_cmd first (unless skip_verify), refusing to complete on a nonzero exit or timeout"
    )]
    fn task_complete(
        &self,
        Parameters(TaskCompleteParams {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
            skip_verify,
        }): Parameters<TaskCompleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let complete_result = complete_task_with_verify(
            &task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
            skip_verify,
        )
        .map_err(|e| to_mcp_error(&e))?;
        ok_json(&complete_result)
    }

    #[tool(
        description = "Advisory, read-only dry run of a task's verify_cmd, without completing it"
    )]
    fn task_verify(
        &self,
        Parameters(TaskIdParams { task_id }): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let (task_id, result) = verify_task_dry_run(&task_id).map_err(|e| to_mcp_error(&e))?;
        ok_value(&serde_json::json!({ "task_id": task_id.to_string(), "result": result }))
    }

    #[tool(description = "Mark a task as failed")]
    fn task_fail(
        &self,
        Parameters(TaskFailParams {
            task_id,
            reason,
            compact,
            force,
        }): Parameters<TaskFailParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::fail(&task_id, reason, compact, force, &mut db)
            .map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Cancel a task that is no longer needed")]
    fn task_cancel(
        &self,
        Parameters(TaskCancelParams {
            task_id,
            reason,
            cascade,
            force,
        }): Parameters<TaskCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let result = ops::task::cancel(&task_id, reason, "mcp", cascade, force, &mut db)
            .map_err(|e| to_mcp_error(&e))?;
        ok_json(&result)
    }

    #[tool(description = "Retry a failed task: returns it to pending")]
    fn task_retry(
        &self,
        Parameters(TaskForceParams { task_id, force }): Parameters<TaskForceParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::retry(&task_id, force, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Submit an in_progress task for review (moves it to verifying)")]
    fn task_submit(
        &self,
        Parameters(TaskForceParams { task_id, force }): Parameters<TaskForceParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::submit(&task_id, force, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Accept a task under review, completing it (verifying to completed)")]
    fn task_accept(
        &self,
        Parameters(TaskAcceptParams {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
        }): Parameters<TaskAcceptParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let complete_result =
            ops::task::accept(&task_id, result, artifacts, tokens, elapsed, force, &mut db)
                .map_err(|e| to_mcp_error(&e))?;
        ok_json(&complete_result)
    }

    #[tool(
        description = "Reject a task under review, sending it back for rework (verifying to in_progress)"
    )]
    fn task_reject(
        &self,
        Parameters(TaskRejectParams {
            task_id,
            reason,
            force,
        }): Parameters<TaskRejectParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task =
            ops::task::reject(&task_id, reason, force, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Release a claimed task back to pending")]
    fn task_release(
        &self,
        Parameters(TaskForceParams { task_id, force }): Parameters<TaskForceParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::release(&task_id, force, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Release all tasks that have been in progress longer than a duration")]
    fn task_release_stale(
        &self,
        Parameters(TaskReleaseStaleParams { duration }): Parameters<TaskReleaseStaleParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let threshold =
            radial_core::duration::parse_duration(&duration).map_err(|e| to_mcp_error(&e))?;
        let tasks = ops::task::release_stale(threshold, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&tasks)
    }

    #[tool(description = "Release every in-progress task regardless of duration (hard reset)")]
    fn task_release_all_in_progress(&self) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let tasks = ops::task::release_all_in_progress(&mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&tasks)
    }

    #[tool(description = "Delete a pending task")]
    fn task_delete(
        &self,
        Parameters(TaskIdParams { task_id }): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::delete(&task_id, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "Add a comment to a task")]
    fn task_comment(
        &self,
        Parameters(TaskCommentParams { task_id, text }): Parameters<TaskCommentParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::comment(&task_id, text, &mut db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "View all comments on a task")]
    fn task_comments(
        &self,
        Parameters(TaskIdParams { task_id }): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let task = ops::task::comments(&task_id, &db).map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(
        description = "Edit a task's description, priority, contract, dependencies, and/or tags. Each field left unset is unchanged"
    )]
    fn task_edit(
        &self,
        Parameters(TaskEditParams {
            task_id,
            description,
            priority,
            receives,
            produces,
            verify,
            verify_cmd,
            clear_verify_cmd,
            add_blocked_by,
            remove_blocked_by,
            set_blocked_by,
            tags,
        }): Parameters<TaskEditParams>,
    ) -> Result<CallToolResult, McpError> {
        let (mut db, _guard) = ensure_initialized_for_write().map_err(|e| to_mcp_error(&e))?;
        let task_id = resolve_task(&db, &task_id)?;
        let priority = parse_priority(priority)?;
        let add_blocked_by = resolve_opt_task_ids(&db, add_blocked_by)?;
        let remove_blocked_by = resolve_opt_task_ids(&db, remove_blocked_by)?;
        let set_blocked_by = resolve_opt_task_ids(&db, set_blocked_by)?;
        let task = ops::edit::task(
            &task_id,
            description,
            priority,
            receives,
            produces,
            verify,
            verify_cmd,
            clear_verify_cmd,
            add_blocked_by,
            remove_blocked_by,
            set_blocked_by,
            tags,
            &mut db,
        )
        .map_err(|e| to_mcp_error(&e))?;
        ok_json(&task)
    }

    #[tool(description = "List a goal's unblocked, ready-to-start tasks")]
    fn ready(
        &self,
        Parameters(ReadyParams {
            goal_id,
            priority,
            tag_filter,
        }): Parameters<ReadyParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let goal_id =
            resolve_goal_or_default(goal_id.as_deref(), &db).map_err(|e| to_mcp_error(&e))?;
        let priority = parse_priority(priority)?;
        let ready = ops::ready::run(&goal_id, priority.as_ref(), &tag_filter, &db)
            .map_err(|e| to_mcp_error(&e))?;
        let value: Vec<serde_json::Value> = ready
            .into_iter()
            .map(|(task, parent)| serde_json::json!({ "task": task, "parent_task": parent }))
            .collect();
        ok_json(&value)
    }

    #[tool(
        description = "Summarize goal and task progress, optionally scoped to a goal, a task, or an assignee"
    )]
    fn status(
        &self,
        Parameters(StatusParams {
            goal_id,
            task_id,
            assignee,
            tag_filter,
        }): Parameters<StatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let goal_id = goal_id.map(|g| resolve_goal(&db, &g)).transpose()?;
        let task_id = task_id.map(|t| resolve_task(&db, &t)).transpose()?;
        let result = ops::status::run(goal_id, task_id, assignee, &tag_filter, &db)
            .map_err(|e| to_mcp_error(&e))?;
        let value = match result {
            ops::status::StatusResult::Task(task) => serde_json::to_value(task),
            ops::status::StatusResult::Goal(goal_status) => serde_json::to_value(goal_status),
            ops::status::StatusResult::AllGoals(summaries) => serde_json::to_value(summaries),
        }
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        ok_value(&value)
    }

    #[tool(
        description = "Get the next unblocked task to work on, with its full contract, parent goal, and dependency context (the tasks it was blocked by) in one call. Does not claim it -- call task_start afterward to claim it"
    )]
    fn get_my_work(
        &self,
        Parameters(GetMyWorkParams {
            goal_id,
            priority,
            tags,
        }): Parameters<GetMyWorkParams>,
    ) -> Result<CallToolResult, McpError> {
        let (db, _guard) = ensure_initialized_for_read().map_err(|e| to_mcp_error(&e))?;
        let goal_id =
            resolve_goal_or_default(goal_id.as_deref(), &db).map_err(|e| to_mcp_error(&e))?;
        let priority = parse_priority(priority)?;
        let mut ready = ops::ready::run(&goal_id, priority.as_ref(), &tags, &db)
            .map_err(|e| to_mcp_error(&e))?;

        let Some((task, parent_task)) = ready.drain(..).next() else {
            return ok_value(&serde_json::json!({
                "task": null,
                "message": "No tasks are ready to start right now.",
            }));
        };

        let goal = db.get_goal(&goal_id);
        let dependencies: Vec<&Task> = task
            .blocked_by()
            .iter()
            .filter_map(|id| db.get_task(id))
            .collect();

        ok_value(&serde_json::json!({
            "task": task,
            "parent_task": parent_task,
            "goal": goal,
            "dependencies": dependencies,
        }))
    }
}

// The macro generates async call_tool/list_tools/get_tool methods per the
// ServerHandler trait's own signature, even though every #[tool] above is
// synchronous -- nothing to await.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler(
    router = self.tool_router,
    instructions = "radial: task orchestration for LLM agents, contract-based coordination. \
    Tools mirror the `rd` CLI 1:1 (goal_*, task_*, ready, status, show). Call get_my_work to \
    find the next unblocked task with its full contract and dependency context before claiming \
    it with task_start; finish with task_complete (runs the contract's verify_cmd first, if set) \
    or the submit/accept/reject review flow."
)]
impl ServerHandler for RadialMcp {}

/// Serves the MCP server over stdio until the client disconnects.
pub(crate) async fn run(allowed_tools: Option<HashSet<String>>) -> anyhow::Result<()> {
    let service = RadialMcp::new(allowed_tools.as_ref())
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
