//! Antigravity (`agy`). Flags are checked against `agy --help`.
//!
//! Antigravity runs prompts non-interactively with `--output-format stream-json -p <prompt>`.
//! The adapter deliberately preserves Antigravity's own permission configuration. It never forces
//! `always-proceed`: that flag approves every tool call, bypassing the person's permission policy.

use super::*;

pub struct Antigravity;

impl AgentAdapter for Antigravity {
    fn ids(&self) -> &'static [&'static str] {
        &["antigravity", "agy"]
    }

    fn args(&self, cx: &ArgsCx) -> Vec<String> {
        let mut a = cx.options.args.clone();
        // Do not enable `--dangerously-skip-permissions` for builds. It sets `always-proceed`
        // and silently bypasses the permissions configured by the person in Antigravity.
        if let Some(m) = &cx.options.cli_model {
            a.extend(["--model".into(), m.clone()]);
        }
        if let Some(r) = cx.resume {
            a.extend(["--conversation".into(), r.to_string()]);
        }
        // Stream input avoids command-line length limits (notably on Windows) and matches the
        // CLI's documented one-message NDJSON input protocol.
        a.extend(["--input-format".into(), "stream-json".into(), "--output-format".into(), "stream-json".into()]);
        a
    }

    fn prompt_on_stdin(&self) -> bool {
        true
    }

    fn stdin_text(&self, preamble: &str, prompt: &str) -> String {
        let message = format!("{preamble}{prompt}");
        format!("{}\n", json!({"event": "user", "message": {"content": message}}))
    }

    fn required_flags(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&["--help"], &["--input-format", "--output-format", "stream-json"])
    }

    fn streams_json(&self) -> bool {
        true
    }

    fn resumes(&self) -> bool {
        true
    }

    /// The headless CLI has no verified, repository-enforced read-only/fork path for questions.
    /// Keep questions refused by the shared runner rather than trusting `--mode plan`.
    fn answers_questions(&self) -> bool {
        false
    }

    /// The headless CLI has no verified, repository-enforced read-only/fork path for questions.
    /// Keep questions refused by the shared runner rather than trusting `--mode plan`.
    fn answers_questions(&self) -> bool {
        false
    }

    fn parse(&self, p: &mut StreamParser, v: &Value) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        let event = v.get("event").and_then(Value::as_str).unwrap_or_default();
        match event {
            "init" => {
                if let Some(id) = v.get("conversation_id").and_then(Value::as_str) {
                    p.session_id = Some(id.to_string());
                }
            }
            "step_update" => {
                let Some(su) = v.get("step_update") else { return out };
                if let Some(id) = su.get("conversation_id").and_then(Value::as_str) {
                    p.session_id.get_or_insert_with(|| id.to_string());
                }
                let step_type = su.get("step_type").and_then(Value::as_str).unwrap_or_default();
                let state = su.get("state").and_then(Value::as_str).unwrap_or_default();
                match step_type {
                    "agent_response" => {
                        if let Some(text) = su.get("text_delta").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                            out.push(AgentEvent::TextDelta { text: text.to_string() });
                        }
                    }
                    "tool" => {
                        let id = su.get("step_index").map(|i| i.to_string()).unwrap_or_default();
                        let name = su.get("tool_name").or_else(|| su.pointer("/tool_info/name")).and_then(Value::as_str).unwrap_or_default();
                        if state == "ACTIVE" {
                            let input = su.pointer("/tool_info/parameters").cloned().unwrap_or(Value::Null);
                            out.push(p.started(&id, name, input));
                        } else if state == "DONE" || state == "ERROR" {
                            if !p.tool_names.contains_key(&id) {
                                let input = su.pointer("/tool_info/parameters").cloned().unwrap_or(Value::Null);
                                out.push(p.started(&id, name, input));
                            }
                            let ok = state != "ERROR" && su.pointer("/tool_info/error").is_none();
                            let text = su.pointer("/tool_info/output").or_else(|| su.pointer("/tool_info/error")).map(content_text).unwrap_or_default();
                            out.push(p.finished(&id, ok, &text));
                        }
                    }
                    _ => {}
                }
            }
            "result" => {
                p.saw_result = true;
                if let Some(res) = v.get("result") {
                    if let Some(u) = res.get("usage") {
                        // Antigravity reports cache-read and thinking counts as breakdowns of
                        // input_tokens/output_tokens, not extra tokens. Adding them again
                        // overstates usage, so retain the documented aggregate counters only.
                        p.input_tokens = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                        p.output_tokens = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                    }
                    if res.get("status").and_then(Value::as_str) == Some("ERROR") {
                        let why = res.get("error").or_else(|| res.get("response")).and_then(Value::as_str).unwrap_or("run failed");
                        out.push(AgentEvent::Error { message: format!("Antigravity: {why}"), remediation: None, log: None });
                    }
                }
            }
            "error" => {
                let msg = v.pointer("/error/message").or_else(|| v.get("error")).and_then(Value::as_str).unwrap_or("unknown error");
                out.push(AgentEvent::Error { message: format!("Antigravity: {msg}"), remediation: None, log: None });
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(builder: bool, resume: Option<&str>, options: &BackendOptions) -> Vec<String> {
        let mcp = McpCommand { command: "craft-anything-cli".into(), args: vec!["mcp".into()], notes: None };
        Antigravity.args(&ArgsCx { options, prompt: "analyse repo", preamble: "PRE ", mcp: &mcp, resume, builder, reads_checked: false, question: false })
    }

    #[test]
    fn antigravity_args_construction() {
        let default_opts = BackendOptions::default();
        assert_eq!(args(false, None, &default_opts), ["--input-format", "stream-json", "--output-format", "stream-json"]);

        let builder_opts =
            BackendOptions { cli_model: Some("Gemini 3.8 Flash (Medium)".into()), args: vec!["--add-dir".into(), "/repo".into()], ..Default::default() };
        assert_eq!(
            args(true, Some("conv-123"), &builder_opts),
            [
                "--add-dir",
                "/repo",
                "--model",
                "Gemini 3.8 Flash (Medium)",
                "--conversation",
                "conv-123",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json"
            ]
        );
        assert!(Antigravity.resumes());
        assert!(!Antigravity.answers_questions(), "questions stay blocked until a real read-only/fork contract exists");
        assert!(!args(true, None, &builder_opts).iter().any(|arg| arg == "--dangerously-skip-permissions"));
        assert!(Antigravity.prompt_on_stdin());
        let input: Value = serde_json::from_str(&Antigravity.stdin_text("PRE ", "analyse repo")).unwrap();
        assert_eq!(input["event"], "user");
        assert_eq!(input["message"]["content"], "PRE analyse repo");
        assert_eq!(Antigravity.required_flags(), (&["--help"][..], &["--input-format", "--output-format", "stream-json"][..]));
        assert!(AgentAdapters::built_in().for_agent("antigravity").ids().contains(&"antigravity"));
        assert!(AgentAdapters::built_in().for_agent("agy").ids().contains(&"agy"));
    }

    #[test]
    fn antigravity_stream_parsing() {
        let adapter: Arc<dyn AgentAdapter> = Arc::new(Antigravity);
        let mut p = StreamParser::new(adapter);

        let init = json!({
            "event": "init",
            "conversation_id": "conv-456",
            "init": {"cwd": "/tmp", "permission_mode": "always-proceed"}
        });
        assert!(p.line(&init.to_string()).is_empty());
        assert_eq!(p.session_id.as_deref(), Some("conv-456"));

        let text_step = json!({
            "event": "step_update",
            "step_update": {
                "conversation_id": "conv-456",
                "step_index": 1,
                "state": "ACTIVE",
                "step_type": "agent_response",
                "text_delta": "Hello from Antigravity!"
            }
        });
        let evs = p.line(&text_step.to_string());
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::TextDelta { text } => assert_eq!(text, "Hello from Antigravity!"),
            other => panic!("expected TextDelta, got {other:?}"),
        }

        let tool_active = json!({
            "event": "step_update",
            "step_update": {
                "step_index": 2,
                "state": "ACTIVE",
                "step_type": "tool",
                "tool_name": "run_command",
                "tool_info": {
                    "name": "run_command",
                    "parameters": {"CommandLine": "cargo check"}
                }
            }
        });
        let evs = p.line(&tool_active.to_string());
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::ToolStarted { call_id, tool, input } => {
                assert_eq!(call_id, "2");
                assert_eq!(tool, "run_command");
                assert_eq!(input["CommandLine"], "cargo check");
            }
            other => panic!("expected ToolStarted, got {other:?}"),
        }

        let tool_done = json!({
            "event": "step_update",
            "step_update": {
                "step_index": 2,
                "state": "DONE",
                "step_type": "tool",
                "tool_name": "run_command",
                "tool_info": {
                    "name": "run_command",
                    "parameters": {"CommandLine": "cargo check"},
                    "output": "Finished dev profile"
                }
            }
        });
        let evs = p.line(&tool_done.to_string());
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::ToolFinished { call_id, tool, ok, summary, .. } => {
                assert_eq!(call_id, "2");
                assert_eq!(tool, "run_command");
                assert!(ok);
                assert_eq!(summary, "Finished dev profile");
            }
            other => panic!("expected ToolFinished, got {other:?}"),
        }

        let result = json!({
            "event": "result",
            "result": {
                "conversation_id": "conv-456",
                "status": "SUCCESS",
                "response": "Done!",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "thinking_tokens": 10,
                    "cache_read_tokens": 20,
                    "total_tokens": 180
                }
            }
        });
        let evs = p.line(&result.to_string());
        assert!(evs.is_empty());
        assert!(p.saw_result);
        assert_eq!(p.input_tokens, 100, "cache_read_tokens is an input breakdown, not an additional count");
        assert_eq!(p.output_tokens, 50, "thinking_tokens is an output breakdown, not an additional count");

        let err = json!({
            "event": "error",
            "error": "authentication expired"
        });
        let evs = p.line(&err.to_string());
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Error { message, .. } => assert!(message.contains("authentication expired")),
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
