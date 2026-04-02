# oxshell

AI coding assistant for the terminal — powered by Cloudflare Workers AI + minimemory, built in Rust.

[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](#)
[![Workers AI](https://img.shields.io/badge/Workers_AI-F38020?logo=cloudflare&logoColor=white)](#)
[![Tests](https://img.shields.io/badge/tests-66_passing-brightgreen)](#tests)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## What is it

oxshell is a terminal-based AI assistant that can read files, write code, run commands, search codebases, execute declarative workflows, and orchestrate multiple agents — all through natural language. It runs on Cloudflare Workers AI (no OpenAI/Anthropic key needed) and stores memories locally using [minimemory](https://github.com/MauricioPerera/minimemory).

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

$ oxshell --resume
[resumed session: a1b2c3d4 — 12 messages]
```

## Install

```bash
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
```

**Config resolution priority**: CLI flags > environment variables > `~/.oxshell/config.json`

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

# Resume previous session
oxshell --resume               # Most recent session
oxshell --resume abc123        # By session ID/prefix

# List sessions
oxshell sessions

# Auto-approve tools (dangerous)
oxshell --auto-approve -p "Fix all lint errors"
```

### TUI Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/skills` | List available skills |
| `/memory` | Show memory stats |
| `/sessions` | List recent sessions |
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
oxshell v1.0 (~11,500 LOC Rust, 75 source files)
├── a2e/           Native A2E executor (8 declarative operations)
├── cli/           CLI args + setup/sessions/doctor subcommands + --resume
├── compaction/    Auto context compaction (model-aware limits)
├── config/        Setup wizard + ~/.oxshell/config.json
├── context/       System prompt builder (memory + skills + coordinator)
├── llm/           Cloudflare Workers AI (streaming, retry, multi-model)
├── mcp/           MCP client (stdio transport, auto-discovery)
├── memory/        Persistent typed memories (minimemory, BM25 + vector, RRF)
├── permissions/   RBAC (auto-approve, session, always, input validation)
├── plugins/       Plugin system (manifest, discovery, registry)
├── session/       JSONL session persistence + resume
├── skills/        Reusable prompts (SKILL.md, bundled + custom, inline/fork)
├── storage/       Conversation history (minimemory)
├── tasks/         Background tasks + sub-agents + coordinator mode
├── theme/         5 color themes (dark, light, solarized, monokai, nord)
├── tools/         12 tools (bash, file_*, glob, grep, skill, a2e, task_*)
├── ui/            ratatui TUI (streaming, tool approvals, task notifications)
├── vim/           Vim mode (motions, operators, state machine)
└── voice/         Voice input (audio capture + Whisper STT)
```

## Features

### Tools (12)

| Tool | Description |
|------|-------------|
| `bash` | Shell commands (blocked patterns, evasion detection, timeout) |
| `file_read` | Read files with line numbers (path traversal protection) |
| `file_write` | Write/create files (symlink protection, sensitive path blocking) |
| `file_edit` | Exact string replacement in files |
| `glob` | File search (.gitignore-aware via `ignore` crate) |
| `grep` | Regex content search (.gitignore-aware, binary detection) |
| `skill` | Invoke registered skills |
| `a2e_execute` | Native declarative workflows (8 operations, no external server) |
| `spawn_agent` | Spawn background sub-agent (coordinator mode) |
| `spawn_bash` | Background shell command (coordinator mode) |
| `task_list` | List all tasks with status |
| `task_stop` | Kill a running task |

### Memory System

Inspired by Claude Code's KAIROS, but using local vector search instead of LLM calls:

- **5 memory types**: user, feedback, project, reference, session
- **Hybrid retrieval**: BM25 keyword + vector similarity with RRF fusion
- **Auto-extraction**: Detects preferences, corrections, decisions, references from conversation
- **Secret detection**: Automatically skips API keys, passwords, tokens in extraction
- **Session summaries**: Automatic end-of-session notes
- **Freshness warnings**: Memories >7 days flagged for verification
- **Zero API cost**: All search is local (<1ms, no Sonnet calls like KAIROS)
- **MEMORY.md index**: Human-readable table of contents (200 line / 25KB cap)
- **Auto-consolidation**: Expires old memories, prunes sessions, trims to 500 max

### Session Persistence

Conversations are automatically saved and can be resumed:

- **JSONL format**: Human-readable session files at `~/.oxshell/sessions/{date}/{id}.jsonl`
- **Auto-title**: First user message becomes session title
- **Resume**: `oxshell --resume` picks up where you left off
- **Session index**: `oxshell sessions` lists recent sessions
- **Compaction-aware**: Compacted sessions preserve summary markers

### Context Compaction

Automatically prevents context window overflow:

- **Model-aware**: Knows limits for 10+ Workers AI models (4K–256K)
- **Auto-trigger**: Activates at 80% of context limit
- **Smart summarization**: Sends old messages to the same model for structured summary
- **Preserves recent**: Keeps last 6 messages intact (tool call pairs stay coherent)
- **Session rewrite**: Updates JSONL file after compaction
- **Re-compaction safe**: `[Session context]` markers handled in subsequent compactions

### Skills System

```bash
# Built-in skills
/commit         # AI-powered git commit
/review         # Code review
/simplify       # Code quality review

# Custom skills — create .oxshell/skills/<name>/SKILL.md
/create-skill   # Meta-skill that creates other skills
```

Skills support: YAML frontmatter, argument substitution (`$1`, `$ARGUMENTS`, `${SKILL_DIR}`), inline/fork execution modes, tool allowlists, conditional activation via path patterns.

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

Workers run independently with their own conversation loops, tool access, and cancellation tokens. The coordinator tracks lifecycle (pending → running → completed/failed/killed) and receives XML notifications.

### Native A2E Executor

Execute declarative workflows inline — no external server needed:

```jsonl
{"type":"operationUpdate","operationId":"fetch","operation":{"ApiCall":{"method":"GET","url":"https://api.example.com/data","outputPath":"/workflow/data"}}}
{"type":"operationUpdate","operationId":"filter","operation":{"FilterData":{"inputPath":"/workflow/data","conditions":[{"field":"status","operator":"==","value":"active"}],"outputPath":"/workflow/active"}}}
{"type":"beginExecution","executionId":"exec-1","operationOrder":["fetch","filter"]}
```

**8 operations**: ApiCall, FilterData, TransformData, Conditional, Loop, StoreData, Wait, MergeData. Supports recursive operations (conditionals and loops), validation before execution, and structured result output.

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

Auto-discovers tools, prefixes with `mcp__servername__toolname`, graceful degradation on server failure.

### Security

- **Path traversal protection**: Canonicalization + blocked sensitive paths
- **Command blocking**: Dangerous patterns + evasion detection (eval, hex, shell expansion)
- **Input validation**: Sensitive file write blocking (.env, .ssh, credentials, cloud configs)
- **Secret detection**: API keys, tokens, passwords automatically excluded from memory extraction
- **Permission system**: Auto-approve / session-approve / always-approve with per-tool granularity
- **Credential privacy**: API tokens stored privately, never logged, HTTPS warnings

## Models Tested

Tested across 10+ Workers AI models. Tool call normalization handles 3 formats automatically: OpenAI standard, Qwen `<tools>` tags, and Granite double-escaped JSON.

| # | Model | Context | Tool Calling | Pricing (in/out per M) | Notes |
|---|-------|---------|-------------|----------------------|-------|
| 🥇 | `@cf/mistralai/mistral-small-3.1-24b-instruct` | 128K | ✅ Native | $0.35 / $0.56 | **Best tool calling** — completes tasks cleanly |
| 🥈 | `@cf/nvidia/nemotron-mini-128k-instruct-3-120b` | 256K | ✅ Native | $0.50 / $1.50 | Best reasoning, stops properly |
| 🥉 | `@cf/ibm-granite/granite-4.0-h-micro` | 131K | ✅ Native | — | Best price/quality ratio |
| 4 | `@cf/moonshotai/kimi-k2.5` | 256K | ✅ Native | $0.60 / $3.00 | Reasoning model, but loops on tools |
| 5 | `@cf/openai/gpt-oss-120b` | 128K | ✅ Native | $0.35 / $0.75 | Reasoning, but loops on tools |
| 6 | `@cf/openai/gpt-oss-20b` | 128K | ✅ Native | $0.20 / $0.30 | Lightweight reasoning, loops on tools |
| 7 | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` | 24K | ✅ Native | $0.29 / $2.25 | Good model, but loops on tools |
| 8 | `@cf/meta/llama-4-scout-17b-16e-instruct` | 131K | ✅ Native | — | Newest Meta model |
| 9 | `@cf/qwen/qwen2.5-coder-32b-instruct` | 32K | ⚠️ `<tools>` tags | $0.66 / $1.00 | Non-standard format, slow on Workers AI |
| 10 | `@hf/nousresearch/hermes-2-pro-mistral-7b` | ~4K | Basic | Free | Fast, free tier |

> **"Loops on tools"** = model calls tools correctly but doesn't stop after receiving results, hitting MAX_TURNS (15). This is model behavior, not a code bug.

## Tests

138 tests across 12 test files, all passing:

```bash
cargo test    # Run all tests
```

| Test File | Tests | Coverage |
|-----------|-------|----------|
| security_tests | 47 | Bash evasion, path validation, tool call normalization, secret detection |
| a2e_tests | 11 | JSONL parsing, workflow validation, filter conditions |
| store_tests | 13 | Compaction, session types, sandbox limits |
| memory_tests | 10 | Secret detection, freshness, age calculation |
| input_tests | 10 | UTF-8 safe insert/backspace/navigation, emoji |
| skills_tests | 11 | Frontmatter parser, lists, render, split |
| tools_tests | 7 | Bash blocking, evasion detection, permission paths |
| tasks_tests | 6 | ID generation, lifecycle, XML escaping |
| config_tests | 5 | Resolution priority, model fallback |
| llm_tests | 4 | Usage accumulation, cost formatting |

## Dependencies

- **Runtime**: tokio, reqwest, ratatui + crossterm
- **Storage**: [minimemory](https://github.com/MauricioPerera/minimemory) (vector DB, zero external deps)
- **File ops**: ignore (gitignore), globset, regex
- **No external AI SDKs** — direct HTTP to Workers AI

## License

MIT
