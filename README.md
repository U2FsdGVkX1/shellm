# Shellm

English | [简体中文](README_zh.md)

An AI-powered terminal assistant. Describe what you want to do, and let AI generate the shell command for you.

## Installation

```bash
cargo build --release
```

## Quick Start

1. Set your OpenAI API key:

```bash
export OPENAI_API_KEY="your-api-key"
```

2. Run shellm:

```bash
./target/release/shellm
```

3. Use your terminal as usual. When you need help, press `Ctrl+L` to chat with AI.

## How It Works

1. Press `Ctrl+L` to enter chat mode
2. Type your question in natural language
3. AI suggests a command
4. Press `Ctrl+L` to accept, or `Ctrl+C` to cancel

## Example

```
[LLM chat] Type your question. Ctrl+L accepts the command. Ctrl+C exits. Ctrl+R toggles reasoning.
you> find all python files modified in the last 7 days
assistant> Search for recently modified Python files
candidate: find . -name "*.py" -mtime -7
```

## Configuration

Shellm supports configuration via environment variables and/or a TOML config file.

### Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_KEY` | Your API key (required) |
| `OPENAI_MODEL` | Model to use (default: `gpt-4o-mini`) |
| `OPENAI_BASE_URL` | Custom API base URL (default: `https://api.openai.com/v1`) |
| `SHELLM_CONFIG` | Path to custom config file (optional) |

### Config File

Create a config file at `~/.config/shellm/config.toml` to customize shellm's behavior:

```bash
mkdir -p ~/.config/shellm
cp config.example.toml ~/.config/shellm/config.toml
```

#### Example Configuration

```toml
[llm]
model = "gpt-4o-mini"
# api_key = "sk-..."  # Or use OPENAI_API_KEY env var

[prompt]
# Custom prompt template with dynamic variables:
#   {os}    - Operating system (Linux, Windows, macOS)
#   {arch}  - CPU architecture (x86_64, aarch64, riscv64, etc.)
#   {shell} - Current shell (bash, zsh, fish, powershell, cmd)
#   {lang}  - Preferred language (zh-CN, en-US, etc.)
template = """
You are a focused shell copilot on {os} ({arch}) running {shell}.
Please answer in {lang}.
Always respond ONLY with a JSON object:
{"command": "<shell command>", "answer": "brief human-readable note"}.
"""

[shell]
# path = "/bin/zsh" # Optional: manually specify shell executable path

[preference]
language = "en-US"  # Or auto-detect from LANG env var
```

### Config Priority

1. Config file settings take priority over environment variables
2. If no config file exists, environment variables are used
3. Default values are used as fallback

## License

GPL-3.0
