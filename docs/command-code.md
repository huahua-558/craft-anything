# Command Code

Craft Anything can use the installed Command Code CLI for a deep build through the Agent Client
Protocol (ACP). This is separate from an OpenAI-compatible server: it runs your own
`command-code` program, signed in with your own account.

## Setup

1. Install Command Code. Its program is `command-code`, or `cmd`, `commandcode`, `cmdc` for the
   same file. Craft Anything finds it on `PATH`.
2. Sign in with `command-code login`. Command Code keeps that login in
   `~/.commandcode/auth.json`. Craft Anything checks whether the file exists and does not read it
   or sign in for you. A missing file is unknown, so a build still starts and reports a sign-in
   problem itself; it is not treated as signed out.
3. In Settings → AI, detect and add **Command Code**, then choose it for **Build with this**.
   A chat and a question are refused. If `command-code` is installed somewhere else, use
   **Choose...** and pick the program.

The equivalent backend entry in `config.toml` is:

```toml
[[ai.backends]]
name = "Command Code"
kind = "agent_cli"
model = "command-code"
enabled = true
```

`model` names the agent. ACP is on for this agent. Do not set `acp = false` or an SSH host:
those are refused rather than opening the interactive session, which has no way to ask Craft
Anything for approval. Extra `args` are refused too: they would be placed before `acp`, where
`--yolo` or `--permission-mode dont-ask` would bypass the approval policy. There is no folder
override: the run uses the sign-in Command Code already has on this computer.

## What runs

A deep build starts:

```sh
command-code acp
```

ACP is the only mode that works, and that is not a preference. Command Code's headless print
mode (`-p --output-format json`) has no flag or variable that hands it an MCP server, so a build
there could not reach Craft Anything's tools at all. `acp` takes the server in `session/new`.

The model and effort are set with `session/set_config_option` after the session opens, the model
first, because changing it can reset the effort.

## Model and thinking

Use the shared model and **Thinking depth** picker (Settings → AI → **Change...**), or set
`cli_model` and `cli_thinking` on the same backend entry.

Command Code accepts four effort levels: `off`, `low`, `high` and `max` (plus its own `default`,
which means "leave it alone"). It refuses anything else with `Unknown effort`, so the shared
levels map onto those: `minimal` and `none` become `off`, `medium` becomes `high`, and `xhigh`
becomes `max`. The picker offers exactly the four translated levels for the selected model, and a
chosen level is validated after translation, so every shared level can be picked. There is no
context-window control.

```toml
cli_model = "claude-sonnet-5-5"
cli_thinking = "high"
```

Discovery sends no prompt. It does open a session long enough to read the model list and apply
`cli_model`, so the thinking choices match that model.

## Tools, MCP and Clean room

Builds attach Craft Anything's MCP server on `session/new` as a stdio server named
`craft-anything`. A live run listed all 326 tools. Command Code keeps the MCP servers from your
own configuration (its global `~/.commandcode/mcp.json` and per-project `mcp.json`); Craft
Anything does not edit those and adds ours for the session only.

Clean room, deep chat and questions are refused before Command Code starts. **An edit or a
command asks permission; a read does not.** That difference is why the rest cannot be allowed: a
question must not change files, and Clean room must keep the original's files out of reach, but
nothing gates a read, so neither can be held. A deep build is allowed: it may read, and an edit
or a command is gated by `session/request_permission`, which reaches the same approval policy
Claude Code's approvals do.

Craft Anything does not pass a flag that approves everything, and it does not override a saved
Command Code setting that does. If your Command Code configuration approves every tool, an edit
can run without a permission request; Craft Anything does not change that configuration to force
a prompt.

## What was checked

Against Command Code 1.91.1: `initialize` (protocol 1, `loadSession`, image prompts, one
`command-code-cli` auth method), `session/new` (session id, five permission modes and the `model`
and `effort` config options), and `session/set_config_option` for both. The four accepted effort
values were checked one by one; `medium`, `xhigh`, `minimal` and an unknown level are each
refused with `-32602 Unknown effort`. The permission round trip was checked directly: a write
sent `session/request_permission` with `allow_once`, `allow_always`, `reject_once` and
`reject_always`, and a read sent nothing. The argument building, the effort mapping, the refused
access modes, the session config order and the account path are covered by unit tests.

A live deep build was run on 2026-11-20 (Command Code 1.91.1, Linux, headless CLI). It attached
the server (326 tools), edited files under the project folder and finished the build stage: the
generated Rust/egui app compiled. The check after it needed a display the machine did not have,
so the run stopped there; that is unrelated to the adapter. The person's own MCP configuration
was not changed.
