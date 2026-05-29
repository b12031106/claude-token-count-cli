# ctc — Claude Token Counter

A fast CLI tool to count tokens in files using Claude's [Token Counting API](https://platform.claude.com/docs/en/build-with-claude/token-counting). Built with Rust.

## Features

- Count tokens for any text file, image, or PDF
- Support multiple files in a single command
- Choose output format: text, JSON, or CSV
- Dynamic model list fetched from Anthropic's Models API
- Persistent config — set API key and default model once

## Install

### Homebrew (macOS / Linux)

```bash
brew install b12031106/tap/ctc
```

### Download prebuilt binary

Download the latest binary from [GitHub Releases](https://github.com/b12031106/claude-token-count-cli/releases), then:

```bash
tar xzf ctc-*.tar.gz
mv ctc ~/.local/bin/   # or anywhere in your $PATH
```

### Cargo (from source)

```bash
cargo install --git https://github.com/b12031106/claude-token-count-cli.git
```

### Build from source

```bash
git clone https://github.com/b12031106/claude-token-count-cli.git
cd claude-token-count-cli
cargo build --release
cp target/release/ctc ~/.local/bin/
```

## Quick Start

```bash
# Set up API key (one-time)
ctc init

# Count tokens in a file
ctc src/main.rs

# Count multiple files
ctc src/main.rs Cargo.toml README.md
```

## Usage

```
ctc [OPTIONS] [FILES]...
ctc <COMMAND>
```

### Commands

| Command | Description |
|---------|-------------|
| `ctc init` | Interactive setup — saves API key to `~/.config/ctc/api_key` |
| `ctc model` | List available models from API and set the default |
| `ctc compare [FILES]...` | Compare token counts of the same content across multiple models |

#### `ctc compare`

Counts the same content under several models side by side — handy for seeing how
tokenization differs between model families (e.g. Opus vs Sonnet/Haiku).

| Flag | Description | Default |
|------|-------------|---------|
| `-m, --models <A,B,C>` | Comma-separated models to compare | latest of each tier (opus / sonnet / haiku), fetched from the API |
| `-s, --system <SYSTEM>` | Include a system prompt in the count | — |
| `-o, --output <FORMAT>` | Output format: `text`, `json`, `csv` | `text` |

```bash
# Compare the latest opus / sonnet / haiku automatically
ctc compare report.pdf

# Compare specific models across several files
ctc compare -m claude-opus-4-8,claude-sonnet-4-6 a.txt b.txt
```

```
File           claude-opus-4-8  claude-sonnet-4-6  claude-haiku-4-5
a.txt                      105                 75                75
b.txt                       17                 16                16
───────────────────────────────────────────────────────────────
TOTAL                      122                 91                91
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-m, --model <MODEL>` | Override model for this run | saved default or `claude-sonnet-4-6` |
| `-s, --system <SYSTEM>` | Include a system prompt in the token count | — |
| `-o, --output <FORMAT>` | Output format: `text`, `json`, `csv` | `text` |
| `-h, --help` | Print help | — |
| `-V, --version` | Print version | — |

### Supported File Types

| Type | Extensions |
|------|-----------|
| Text | Any UTF-8 file (`.rs`, `.py`, `.ts`, `.json`, `.md`, etc.) |
| Image | `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp` |
| PDF | `.pdf` |

## Examples

```bash
# Specify a different model
ctc -m claude-opus-4-7 src/main.rs

# JSON output (useful for scripting)
ctc -o json src/main.rs Cargo.toml

# CSV output (useful for spreadsheets)
ctc -o csv src/*.rs

# Include system prompt in token count
ctc -s "You are a code reviewer" src/main.rs
```

### Output Formats

**Text** (default):
```
main.rs: 1234 tokens
Cargo.toml: 56 tokens

Total: 1290 tokens
```

**JSON** (`-o json`):
```json
{
  "model": "claude-sonnet-4-6",
  "files": [
    { "file": "main.rs", "tokens": 1234 },
    { "file": "Cargo.toml", "tokens": 56 }
  ],
  "total_tokens": 1290
}
```

**CSV** (`-o csv`):
```
file,tokens
main.rs,1234
Cargo.toml,56
total,1290
```

## Configuration

Config files are stored in `~/.config/ctc/`:

| File | Description |
|------|-------------|
| `api_key` | Anthropic API key (file permission: 600) |
| `model` | Default model name |

API key resolution order:
1. `ANTHROPIC_API_KEY` environment variable
2. `~/.config/ctc/api_key` config file

## API Endpoints Used

| Endpoint | Purpose | Cost |
|----------|---------|------|
| `POST /v1/messages/count_tokens` | Count tokens | Free |
| `GET /v1/models` | List available models | Free |

> Note: Both endpoints are free but require an Anthropic account with valid billing status.

## License

ISC
