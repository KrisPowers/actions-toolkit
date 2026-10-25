# AI agents

An AI model running on your machine, whether local through Ollama or in the cloud through
Anthropic or OpenAI, can run your workflows with ATK, watch them, and review what failed. A
model can't drive a terminal UI, so everything the TUI does is also available as plain
commands and as a set of tools.

There are three ways in, all backed by the same tools and the same run history:

| Way in | Use it when |
| --- | --- |
| `atk mcp` | The agent supports the Model Context Protocol (Claude Code, Claude Desktop, Codex, the OpenAI Agents SDK, MCP-capable Ollama front ends) |
| `atk tools` + `atk call` | You write your own agent loop against a model API (Ollama, OpenAI, Anthropic) |
| `atk <command> --json` | The agent can already run shell commands |

Runs started by an agent show up in `atk runs` and the TUI like any other, and you can follow
one with `atk watch` while the agent works.

## The tools

| Tool | What it does |
| --- | --- |
| `list_workflows` | The project's workflows, their triggers and jobs |
| `validate_workflows` | Parse and plan workflow files without running them |
| `run_workflow` | Start a workflow (or one `job`) in the background; returns the run id |
| `wait_for_run` | Block until a run ends or `timeout_seconds` pass; returns its review and `done` |
| `get_run` | A run's review: outcome, jobs, steps, and each failed step's errors and last output |
| `get_logs` | A page of log lines, optionally for one job and step; pass `next_offset` back to read on |
| `list_runs` | The project's runs, newest first |
| `cancel_run` | Stop a running run |
| `rerun` | Start a past run's workflow again |
| `docker_status` | Whether Docker is ready |

Every tool also takes an optional `project` folder; without it, the folder ATK was started in
is used. Run ids can be `last`. The usual loop is `run_workflow`, then `wait_for_run` until
`done` is true, then `get_run` to review; fix the code, `validate_workflows`, and `rerun`.

## MCP clients

`atk mcp` speaks MCP over stdio. Start it from the project folder, or pass `-C <folder>`.

Claude Code, from the project folder:

```sh
claude mcp add atk -- atk mcp
```

Claude Desktop, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "atk": { "command": "atk", "args": ["-C", "/path/to/project", "mcp"] }
  }
}
```

Codex, in `~/.codex/config.toml`:

```toml
[mcp_servers.atk]
command = "atk"
args = ["mcp"]
```

The OpenAI Agents SDK (Python):

```python
from agents.mcp import MCPServerStdio

atk = MCPServerStdio(params={"command": "atk", "args": ["-C", "/path/to/project", "mcp"]})
```

## Your own agent loop

`atk tools` prints the tool definitions in the shape a model API expects, and `atk call` runs
one, printing its JSON result (or `{"error": ...}` with exit code 1):

```sh
atk tools --format openai      # OpenAI Chat Completions, and Ollama's /api/chat
atk tools --format anthropic   # Anthropic Messages API
atk tools --format mcp         # MCP tools/list entries
atk call run_workflow '{"workflow": "ci", "wait_seconds": 300}'
```

A minimal loop with Ollama:

```python
import json, subprocess, ollama

tools = json.loads(subprocess.check_output(["atk", "tools", "--format", "openai"]))
messages = [{"role": "user", "content": "Run the CI workflow and tell me why it fails."}]
while True:
    reply = ollama.chat(model="qwen3", messages=messages, tools=tools).message
    messages.append(reply)
    if not reply.tool_calls:
        break
    for call in reply.tool_calls:
        args = json.dumps(call.function.arguments)
        result = subprocess.run(["atk", "call", call.function.name, args],
                                capture_output=True, text=True).stdout
        messages.append({"role": "tool", "content": result, "tool_name": call.function.name})
print(reply.content)
```

The same works with OpenAI or Anthropic: send the definitions from `atk tools`, and answer
each tool call with the output of `atk call`.

## Agents with a shell

An agent that can run commands needs nothing extra. Every command takes `--json`:

```sh
atk list --json
atk run ci --detach --json          # {"id": "...", "outcome": "running"}
atk watch last --json               # JSON lines while it runs, then the review; exits 0/1/130
atk status last --json              # the review: jobs, steps, failures
atk logs last --job test --step "Run tests" --tail 50 --json
```
