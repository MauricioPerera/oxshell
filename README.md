# oxshell

AI coding assistant for the terminal — powered by Cloudflare Workers AI + minimemory, built in Rust.

[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](#)
[![Workers AI](https://img.shields.io/badge/Workers_AI-F38020?logo=cloudflare&logoColor=white)](#)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## What is it

oxshell is a terminal-based AI assistant that can read files, write code, run commands, search codebases, execute workflows, and orchestrate multiple agents — all through natural language. It runs on Cloudflare Workers AI (no OpenAI/Anthropic key needed) and stores memories locally using [minimemory](https://github.com/MauricioPerera/minimemory).

```
$ oxshell -p "Find all TODO comments in this project"
[tool: grep]
Found 12 TODO comments across 5 files...

$ oxshell --coordinator -p "Analyze this codebase and suggest improvements"
[spawn_agent: a1b2c3d4 — "Scan source files"]
[spawn_agent: e5f6g7h8 — "Check test coverage"]
[task completed: a1b2c3d4]
[task completed: e5f6g7h8]
Based on my analysis, here are the top 5 improvements...
```

## Install

```bash
# Build from source
git clone https://github.com/MauricioPerera/oxshell.git
cd oxshell
cargo build --release
cp target/release/oxshell ~/.local/bin/
```

## Setup

```bash
# Interactive setup wizard (recommended)
oxshell setup
```

The wizard will:
1. Ask for your Cloudflare API token ([create one here](https://dash.cloudflare.com/profile/api-tokens) with Workers AI permissions)
2. Auto-detect your Account ID
3. Let you choose a default model
4. Test the connection
5. Save config to `~/.oxshell/config.json`

**Manual setup** (alternative):

```bash
export CLOUDFLARE_API_TOKEN="your-token"
export CLOUDFLARE_ACCOUNT_ID="your-account-id"

# Optional: A2E server for declarative workflows
export A2E_SERVER_URL="http://localhost:8000"
export A2E_API_KEY="your-key"
```

## Usage

```bash
# Interactive TUI
oxshell

# Single prompt (pipe mode)
oxshell -p "Explain what src/main.rs does"

# Choose model
oxshell -m "@cf/ibm-granite/granite-4.0-h-micro"

# Coordinator mode (multi-agent)
oxshell --coordinator -p "Refactor the auth module"

# Auto-approve tools (dangerous)
oxshell --auto-approve -p "Fix all lint errors"
```

### TUI Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/skills` | List available skills |
| `/memory` | Show memory stats |
| `/cost` | Token usage |
| `/clear` | Clear conversation |
| `/<skill>` | Run a skill (e.g. `/commit`, `/review`) |
| `/exit` | Quit |

### Keyboard

| Key | Action |
|-----|--------|
| `Enter` | Submit message |
| `Ctrl+C` | Quit |
| `Ctrl+Up/Down` | Input history |
| `Up/Down` | Scroll chat |
| `Esc` | Cancel / clear input |
| `y/n/a` | Approve / deny / always-approve tools |

## Architecture

```
oxshell (6,690 LOC Rust)
├── llm/        Cloudflare Workers AI (streaming, retry, multi-model)
├── tools/      12 tools (bash, file_*, glob, grep, skill, a2e, task_*)
├── memory/     Persistent typed memories (minimemory, BM25 + vector, RRF)
├── skills/     Reusable prompts (SKILL.md, bundled + custom, inline/fork)
├── tasks/      Background tasks + sub-agents + coordinator mode
├── mcp/        MCP client (stdio transport, auto-discovery)
├── ui/         ratatui TUI (streaming, tool approvals, task notifications)
├── permissions/ RBAC (auto-approve, session, always, input validation)
├── storage/    Conversation history (minimemory)
└── context/    System prompt builder (memory + skills + coordinator injection)
```

## Features

### Tools (12)

| Tool | Description |
|------|-------------|
| `bash` | Shell commands (blocked patterns, timeout) |
| `file_read` | Read files with line numbers (path validation) |
| `file_write` | Write/create files (symlink protection) |
| `file_edit` | Exact string replacement in files |
| `glob` | File search (.gitignore-aware via `ignore` crate) |
| `grep` | Regex content search (.gitignore-aware) |
| `skill` | Invoke registered skills |
| `a2e_execute` | Declarative workflows (A2E protocol) |
| `spawn_agent` | Spawn background sub-agent |
| `spawn_bash` | Background shell command |
| `task_list` | List all tasks |
| `task_stop` | Kill a task |

### Memory System

Inspired by Claude Code's KAIROS, but using local vector search instead of LLM calls:

- **5 memory types**: user, feedback, project, reference, session
- **Hybrid retrieval**: BM25 keyword + vector similarity with RRF fusion
- **Auto-extraction**: Detects user preferences, corrections, decisions from conversation
- **Session summaries**: Automatic end-of-session notes
- **Freshness warnings**: Old memories flagged for verification
- **Zero API cost**: All search is local (no Sonnet calls like KAIROS)
- **MEMORY.md index**: Human-readable table of contents (200 line cap)

### Skills System

```bash
# Built-in skills
/commit     # AI-powered git commit
/review     # Code review
/simplify   # Code quality review

# Custom skills — create .oxshell/skills/<name>/SKILL.md
/create-skill  # Meta-skill that creates other skills
```

### Coordinator Mode

Multi-agent orchestration via `--coordinator`:

```
Coordinator → spawns Worker A (research)
            → spawns Worker B (analysis)
            → receives <task-notification> XML
            → synthesizes results
            → spawns Worker C (implementation)
            → responds to user
```

### MCP Client

Connect any MCP server via `.oxshell/mcp.json`:

```json
{
  "servers": {
    "my-server": {
      "command": "node",
      "args": ["path/to/server.js"],
      "env": { "API_KEY": "value" }
    }
  }
}
```

### A2E Integration

Execute declarative workflows without tool calling:

```jsonl
{"type":"operationUpdate","operationId":"fetch","operation":{"ApiCall":{"method":"GET","url":"https://api.example.com/data","outputPath":"/workflow/data"}}}
{"type":"beginExecution","executionId":"exec-1","operationOrder":["fetch"]}
```

## Models Tested

| Model | Context | Tool Calling | Performance |
|-------|---------|-------------|-------------|
| `@hf/nousresearch/hermes-2-pro-mistral-7b` | ~4K | Basic | Fast, free tier |
| `@cf/ibm-granite/granite-4.0-h-micro` | 131K | Native | Best quality, paid |

## Dependencies

- **Runtime**: tokio (async), reqwest (HTTP), ratatui + crossterm (TUI)
- **Storage**: [minimemory](https://github.com/MauricioPerera/minimemory) (vector DB)
- **File ops**: ignore (gitignore), globset, regex
- **No** external AI SDKs — direct HTTP to Workers AI

## License

MIT
