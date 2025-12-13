# Shellm

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
[LLM chat] Type your question. Ctrl+L accepts the command. Ctrl+C exits.
you> find all python files modified in the last 7 days
assistant> Search for recently modified Python files
candidate: find . -name "*.py" -mtime -7
```

## Configuration

| Variable | Description |
|----------|-------------|
| `OPENAI_API_KEY` | Your API key (required) |
| `OPENAI_MODEL` | Model to use (default: `gpt-4o-mini`) |
| `OPENAI_BASE_URL` | Custom API endpoint (optional) |

## License

GPL-3.0
