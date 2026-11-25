//! Drives `rd mcp` as a real subprocess over stdio, speaking JSON-RPC
//! directly -- this is the thing the MCP tool wrappers in `src/mcp.rs`
//! can't prove on their own, since nothing else exercises the actual
//! stdio transport and request/response framing.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

struct McpSession {
    _temp_dir: TempDir,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpSession {
    /// Initializes a fresh `.radial/` directory, then launches `rd mcp`
    /// in it and completes the MCP `initialize` handshake.
    fn start(allow_tools: &[&str]) -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let mut binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        binary_path.push("target");
        binary_path.push("debug");
        binary_path.push("rd");

        let init_status = Command::new(&binary_path)
            .arg("init")
            .current_dir(temp_dir.path())
            .status()
            .expect("failed to run rd init");
        assert!(init_status.success(), "rd init failed");

        let mut args = vec!["mcp".to_string()];
        for tool in allow_tools {
            args.push("--allow-tool".to_string());
            args.push((*tool).to_string());
        }

        let mut child = Command::new(&binary_path)
            .args(&args)
            .current_dir(temp_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn rd mcp");

        let stdin = child.stdin.take().expect("no stdin");
        let stdout = BufReader::new(child.stdout.take().expect("no stdout"));

        let mut session = Self {
            _temp_dir: temp_dir,
            child,
            stdin,
            stdout,
            next_id: 1,
        };

        let init_result = session.request(
            "initialize",
            &json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "mcp_test.rs", "version": "0"},
            }),
        );
        assert!(
            init_result.get("result").is_some(),
            "initialize failed: {init_result:?}"
        );
        session.notify("notifications/initialized", &json!({}));

        session
    }

    fn notify(&mut self, method: &str, params: &Value) {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        writeln!(self.stdin, "{msg}").expect("failed to write notification");
        self.stdin.flush().expect("failed to flush stdin");
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{msg}").expect("failed to write request");
        self.stdin.flush().expect("failed to flush stdin");

        let mut line = String::new();
        let n = self
            .stdout
            .read_line(&mut line)
            .expect("failed to read response");
        assert!(n > 0, "rd mcp closed stdout without responding");
        serde_json::from_str(&line).expect("response was not valid JSON")
    }

    fn list_tools(&mut self) -> Vec<String> {
        let resp = self.request("tools/list", &json!({}));
        resp["result"]["tools"]
            .as_array()
            .expect("tools/list result.tools was not an array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    /// Calls a tool and returns its parsed `data`. Panics on an error
    /// response -- for asserting success paths.
    fn call_ok(&mut self, name: &str, arguments: &Value) -> Value {
        let resp = self.request("tools/call", &json!({"name": name, "arguments": arguments}));
        assert!(
            resp.get("error").is_none(),
            "tool {name} returned an error: {resp:?}"
        );
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} response had no text content: {resp:?}"));
        serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} returned non-JSON text: {e}"))
    }

    /// Calls a tool and returns the raw JSON-RPC response, for asserting
    /// on error paths.
    fn call_raw(&mut self, name: &str, arguments: &Value) -> Value {
        self.request("tools/call", &json!({"name": name, "arguments": arguments}))
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn test_mcp_claim_work_complete_verify_cycle() {
    let mut session = McpSession::start(&[]);

    let tool_names = session.list_tools();
    for expected in [
        "goal_create",
        "task_create",
        "task_start",
        "task_complete",
        "task_verify",
        "get_my_work",
        "ready",
        "status",
        "show",
    ] {
        assert!(
            tool_names.contains(&expected.to_string()),
            "expected tool '{expected}' in list_tools, got {tool_names:?}"
        );
    }

    let goal = session.call_ok(
        "goal_create",
        &json!({"description": "MCP integration test goal"}),
    );
    let goal_id = goal["id"].as_str().unwrap().to_string();
    assert_eq!(goal["state"], "pending");

    let task = session.call_ok(
        "task_create",
        &json!({
            "goal_id": goal_id,
            "description": "MCP integration test task",
            "receives": "nothing",
            "produces": "a passing check",
            "verify": "the verify_cmd exits 0",
            "verify_cmd": "exit 0",
        }),
    );
    let task_id = task["id"].as_str().unwrap().to_string();
    assert_eq!(task["state"], "pending");

    let work = session.call_ok("get_my_work", &json!({"goal_id": goal_id}));
    assert_eq!(
        work["task"]["id"], task_id,
        "get_my_work should surface the only ready task"
    );
    assert!(
        work["dependencies"].as_array().unwrap().is_empty(),
        "task has no blockers, so dependencies should be empty"
    );

    let started = session.call_ok(
        "task_start",
        &json!({"task_id": task_id, "assignee": "mcp-integration-test"}),
    );
    assert_eq!(started["state"], "in_progress");
    assert_eq!(started["assignee"], "mcp-integration-test");

    let verify = session.call_ok("task_verify", &json!({"task_id": task_id}));
    assert_eq!(verify["result"]["passed"], true);

    let completed = session.call_ok(
        "task_complete",
        &json!({"task_id": task_id, "result": "Done via MCP"}),
    );
    assert_eq!(completed["task"]["state"], "completed");

    let status = session.call_ok("status", &json!({"goal_id": goal_id}));
    assert_eq!(status["computed_metrics"]["tasks_completed"], 1);
}

#[test]
fn test_mcp_task_complete_refuses_on_failing_verify_cmd() {
    let mut session = McpSession::start(&[]);

    let goal = session.call_ok(
        "goal_create",
        &json!({"description": "MCP verify-refusal test goal"}),
    );
    let goal_id = goal["id"].as_str().unwrap().to_string();

    let task = session.call_ok(
        "task_create",
        &json!({
            "goal_id": goal_id,
            "description": "task with a failing verify_cmd",
            "verify_cmd": "exit 1",
        }),
    );
    let task_id = task["id"].as_str().unwrap().to_string();

    session.call_ok(
        "task_start",
        &json!({"task_id": task_id, "assignee": "mcp-integration-test"}),
    );

    let resp = session.call_raw(
        "task_complete",
        &json!({"task_id": task_id, "result": "should be refused"}),
    );
    let error = resp
        .get("error")
        .unwrap_or_else(|| panic!("expected an error response, got {resp:?}"));
    assert_eq!(error["data"]["code"], "verify_failed");

    // The task must still be in_progress -- the refused completion must not
    // have been recorded.
    let status = session.call_ok("status", &json!({"task_id": task_id}));
    assert_eq!(status["state"], "in_progress");
}

#[test]
fn test_mcp_allow_tool_scopes_visible_and_callable_tools() {
    let mut session = McpSession::start(&["goal_list", "status"]);

    let tool_names = session.list_tools();
    assert_eq!(
        tool_names,
        vec!["goal_list".to_string(), "status".to_string()],
        "list_tools should only show the allowed tools, sorted"
    );

    let disallowed = session.call_raw("goal_create", &json!({"description": "should be blocked"}));
    assert!(
        disallowed.get("error").is_some(),
        "a disallowed tool must be rejected, got {disallowed:?}"
    );

    let allowed = session.call_raw("goal_list", &json!({}));
    assert!(
        allowed.get("error").is_none(),
        "an allowed tool must succeed, got {allowed:?}"
    );
}
