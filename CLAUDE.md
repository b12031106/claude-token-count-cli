# CLAUDE.md

## Project Overview

`ctc` (Claude Token Counter) is a Rust CLI tool that counts tokens in files using Anthropic's Token Counting API.

## Tech Stack

- **Language**: Rust (edition 2024)
- **HTTP client**: reqwest
- **CLI framework**: clap (derive)
- **Serialization**: serde + serde_json
- **Async runtime**: tokio
- **Error handling**: anyhow

## Project Structure

```
src/main.rs    — Single-file application: CLI parsing, API calls, file handling, config
Cargo.toml     — Dependencies and metadata
```

## Build & Run

```bash
cargo build --release          # Build optimized binary → target/release/ctc
cargo run -- <files...>        # Run in dev mode
cargo run -- init              # Run init subcommand
cargo run -- model             # Run model subcommand
cargo run -- compare <files>   # Compare token counts across models
```

## Architecture

All code lives in `src/main.rs`. Key sections:

- **Config** (`config_dir`, `load_api_key`, `load_default_model`, `save_default_model`): reads/writes `~/.config/ctc/` files
- **CLI** (`Cli`, `CountArgs`, `CompareArgs`, `Commands`): clap-derived structs. Top-level args are for token counting; subcommands for `init`, `model`, and `compare`
- **Compare** (`run_compare`, `pick_default_models`, `print_compare_text`, `Cell`): counts the same content across multiple models. Default model set = newest of each tier (opus/sonnet/haiku) via `/v1/models`, sorted by `created_at`. Reuses `count_tokens_for_content` (content built once per file, cloned per model)
- **API types** (`CountTokensRequest`, `Content`, `ContentBlock`, etc.): serde structs matching Anthropic's API schema
- **File handling** (`build_content`): detects file type by extension, reads as text or base64-encodes for images/PDFs
- **API calls** (`count_tokens`, `fetch_models`): async functions hitting `/v1/messages/count_tokens` and `/v1/models`
- **Output** (`OutputFormat`, `JsonOutput`, `FileResult`): supports text, JSON, CSV output formats

## Anthropic API Endpoints

- `POST /v1/messages/count_tokens` — count tokens (free, rate-limited)
- `GET /v1/models` — list available models (free)

Both require `x-api-key` and `anthropic-version: 2023-06-01` headers.

## Conventions

- No external config file format — plain text files in `~/.config/ctc/` (one value per file)
- API key file is set to permission 0600 on Unix
- Default model: `claude-sonnet-4-6` (overridden by saved config, then by `-m` flag)
- Errors go to stderr, results go to stdout
