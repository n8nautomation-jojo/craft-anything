# Antigravity CLI (`agy`)

Craft Anything has a native adapter for the unmodified Antigravity CLI. It is a direct headless-process integration, not an ACP adapter. Install the official CLI separately and make sure `agy` is available on the account's normal executable search path.

## How a run is started

The adapter uses the CLI's documented newline-delimited JSON input and output modes:

- It sends one `user` event to standard input with `--input-format stream-json`.
- It reads `--output-format stream-json` incrementally. The parser handles `init`, `step_update`, `result`, and `error` events, including response deltas and tool start/finish events.
- The Settings → AI model picker runs `agy models` on the selected local or SSH installation. It groups model slugs into a base model and the effort variants the CLI actually lists. The chosen model and optional effort are passed with `--model` and `--effort`; selections are checked against the live catalogue before saving and again before a run.
- A saved builder conversation is continued with `--conversation`.
- Prompt text is not placed in the operating-system command line; it is serialized as one JSON input line. This avoids the platform command-line length limit for long prompts.

The usage counters shown by Craft Anything use the CLI's aggregate `input_tokens` and `output_tokens`. The separate `cache_read_tokens` and `thinking_tokens` fields are breakdowns and are not added a second time.

See Google's [headless CLI documentation](https://antigravity.google/docs/cli/headless) for the input/output protocol, conversation continuation, flags, and exit statuses.

## Setup and files

Craft Anything's Setup integration registers the `craft-anything` MCP server in Antigravity's global configuration:

- MCP configuration: `~/.gemini/config/mcp_config.json`
- Shared global skills: `~/.gemini/config/skills/`

The shared skills directory is read by both Antigravity CLI and the Antigravity App. The CLI also supports additional CLI-only skill locations, but Craft Anything intentionally uses the shared global location.

The Setup merge is scoped to `mcpServers.craft-anything`; unrelated MCP entries and existing JSONC comments are preserved. The `ANTIGRAVITY_HOME` environment variable changes the home root used to resolve these paths.

Discovery does not treat a generic `~/.gemini/config` directory as proof that Antigravity is installed. It looks for the Antigravity CLI-specific directory or the exact Antigravity MCP configuration file, avoiding a false positive from a Gemini-related folder alone.

See Google's [MCP configuration guide](https://antigravity.google/docs/mcp?tab=cli) and [agent skills guide](https://antigravity.google/docs/skills/) for the current file formats and locations.

## Sign-in and credentials

Headless runs use the CLI's cached credentials. Sign in through the Antigravity App or an interactive `agy` session first. Google documents a unified local sign-in layer shared by the App and CLI; on a remote SSH host, run `agy` on that host and complete its browser URL/code flow. See Google's [authentication instructions](https://antigravity.google/docs/cli/install).

Craft Anything launches the user's installed `agy` executable. It does not extract, copy, persist, or forward Google OAuth tokens, and it does not create a separate Google login flow. In Settings → AI, **Load models** invokes `agy models` with bounded time and output and displays the model choices returned by the CLI. This checks that the CLI command can run and provide its catalogue; it does not inspect tokens or independently prove that a model request will authenticate. Authentication failures during model use are reported by the CLI with a sign-in hint.

## Permission and safety boundaries

The adapter never adds `--dangerously-skip-permissions`. Google's CLI documentation states that this flag switches the permission mode to `always-proceed` and approves all tool calls, including file writes and command execution. The adapter therefore leaves Antigravity's own execution and permission configuration in control.

There is an important headless-mode limitation: an action that requires an interactive approval prompt but cannot obtain one is soft-denied by Antigravity. The process may continue and exit successfully while reporting the denied action on `stderr`. Configure only narrowly scoped permission rules in Antigravity itself when a task requires them; Craft Anything does not rewrite the user's Antigravity permission settings. See Google's [headless permission behaviour](https://antigravity.google/docs/cli/headless) and [fine-grained CLI permissions](https://antigravity.google/docs/permissions?tab=cli).

This adapter is not yet wired into Craft Anything's cross-agent approval contract. Craft Anything therefore refuses to start Antigravity at all while the project is in Clean room; it returns a clear error before launching the CLI. Antigravity can run only after the person explicitly switches the project to Deep mode. Deep mode is not a guarantee that Antigravity's own tools are routed through Craft Anything's approval inbox, so the CLI's own permission rules still matter. Clean room is not an operating-system sandbox. Treat this integration as experimental until the common approval/capability contract in roadmap R2.2 is completed.

## Questions about a build

Antigravity is deliberately not enabled for Craft Anything's read-only project-question workflow. The current CLI supports resuming a conversation with `--conversation`, but the integration has not established a supported way to fork that conversation and enforce read-only access for the question. A `plan` mode is not used as a substitute for an enforceable permission boundary.

## Current limitations

- No separate OAuth-status API or account/token inspection; the connection check uses the actual `agy models` command.
- Model discovery and per-model effort choices come from the installed CLI's output; the app does not maintain a hard-coded model catalogue.
- No read-only project-question support.
- No unified mapping from Craft Anything's Deep-mode approval inbox to Antigravity's native permission ACL; Clean room is blocked before launch.
- No live CLI run has been claimed as part of this change; unit tests and platform builds must be run in the repository CI or a development environment with Rust installed.
