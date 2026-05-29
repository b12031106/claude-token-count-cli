use anyhow::{bail, Context, Result};
use base64::Engine;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const CONFIG_DIR: &str = ".config/ctc";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(CONFIG_DIR)
}

fn config_file(name: &str) -> PathBuf {
    config_dir().join(name)
}

fn ensure_config_dir() -> Result<()> {
    let dir = config_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;
    }
    Ok(())
}

fn load_api_key() -> Result<String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    let path = config_file("api_key");
    if path.exists() {
        let key = fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?
            .trim()
            .to_string();
        if !key.is_empty() {
            return Ok(key);
        }
    }

    bail!(
        "API key not found. Run `ctc init` to set up, or set ANTHROPIC_API_KEY environment variable."
    )
}

fn load_default_model() -> String {
    let path = config_file("model");
    if path.exists() {
        if let Ok(model) = fs::read_to_string(&path) {
            let model = model.trim().to_string();
            if !model.is_empty() {
                return model;
            }
        }
    }
    DEFAULT_MODEL.to_string()
}

fn save_default_model(model: &str) -> Result<()> {
    ensure_config_dir()?;
    let path = config_file("model");
    fs::write(&path, model).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn run_init() -> Result<()> {
    println!("🔑 Claude Token Counter — Setup\n");

    let path = config_file("api_key");
    if path.exists() {
        let existing = fs::read_to_string(&path)?.trim().to_string();
        if !existing.is_empty() {
            let masked = format!(
                "{}...{}",
                &existing[..8.min(existing.len())],
                &existing[existing.len().saturating_sub(4)..]
            );
            println!("Found existing API key: {masked}");
            print!("Overwrite? [y/N] ");
            io::stdout().flush()?;
            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            if !answer.trim().eq_ignore_ascii_case("y") {
                println!("Kept existing key.");
                return Ok(());
            }
        }
    }

    print!("Enter your Anthropic API key: ");
    io::stdout().flush()?;
    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    let key = key.trim();

    if key.is_empty() {
        bail!("API key cannot be empty.");
    }

    ensure_config_dir()?;
    fs::write(&path, key).with_context(|| format!("writing config {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    println!("\n✅ API key saved to {}", path.display());
    println!("   (file permissions set to 600 — only you can read it)");
    println!("\nYou're all set! Try: ctc <file>");
    Ok(())
}

async fn fetch_models(client: &reqwest::Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let mut all_models = Vec::new();
    let mut after_id: Option<String> = None;

    loop {
        let mut url = "https://api.anthropic.com/v1/models?limit=100".to_string();
        if let Some(ref cursor) = after_id {
            url.push_str(&format!("&after_id={cursor}"));
        }

        let response = client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .context("fetching models from Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            if let Ok(err) = response.json::<ApiError>().await {
                bail!("API error ({}): {}", status, err.error.message);
            }
            bail!("API error: {}", status);
        }

        let page: ModelsResponse = response.json().await.context("parsing models response")?;
        all_models.extend(page.data);

        if page.has_more {
            after_id = page.last_id;
        } else {
            break;
        }
    }

    Ok(all_models)
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}

async fn run_model() -> Result<()> {
    let api_key = load_api_key()?;
    let client = reqwest::Client::new();
    let current = load_default_model();

    println!("Fetching models from Anthropic API...\n");
    let models = fetch_models(&client, &api_key).await?;

    if models.is_empty() {
        bail!("No models returned from API.");
    }

    println!("Current default: {current}\n");
    println!("Available models:");

    for (i, m) in models.iter().enumerate() {
        let marker = if m.id == current { " ←" } else { "" };
        let ctx = format_tokens(m.max_input_tokens);
        println!(
            "  {:>2}) {:<40} {} ({}){marker}",
            i + 1,
            m.id,
            m.display_name,
            ctx
        );
    }

    println!(
        "\nEnter number [1-{}] or a custom model name (empty to cancel):",
        models.len()
    );
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        println!("No changes.");
        return Ok(());
    }

    let selected = if let Ok(n) = input.parse::<usize>() {
        if n >= 1 && n <= models.len() {
            models[n - 1].id.clone()
        } else {
            bail!("Invalid selection: {n}");
        }
    } else {
        input.to_string()
    };

    save_default_model(&selected)?;
    println!("\n✅ Default model set to: {selected}");
    Ok(())
}

// --- CLI ---

#[derive(Parser)]
#[command(
    name = "ctc",
    about = "Claude Token Counter — count tokens in files using Claude's API",
    version,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    count: CountArgs,
}

#[derive(Args)]
struct CountArgs {
    /// File paths to count tokens for
    files: Vec<PathBuf>,

    /// Claude model to use (overrides default)
    #[arg(short, long)]
    model: Option<String>,

    /// System prompt to include in token count
    #[arg(short, long)]
    system: Option<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
    output: OutputFormat,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Csv,
}

#[derive(Serialize)]
struct FileResult {
    file: String,
    tokens: u64,
}

#[derive(Serialize)]
struct JsonOutput {
    model: String,
    files: Vec<FileResult>,
    total_tokens: u64,
}

#[derive(Args)]
struct CompareArgs {
    /// File paths to count tokens for
    files: Vec<PathBuf>,

    /// Models to compare, comma-separated (default: latest of each tier)
    #[arg(short, long, value_delimiter = ',')]
    models: Option<Vec<String>>,

    /// System prompt to include in token count
    #[arg(short, long)]
    system: Option<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
    output: OutputFormat,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up your Anthropic API key
    Init,
    /// List available models and set the default
    Model,
    /// Compare token counts of the same content across multiple models
    Compare(CompareArgs),
}

// --- API types ---

#[derive(Serialize)]
struct CountTokensRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: Content,
}

#[derive(Serialize, Clone)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "document")]
    Document { source: DocumentSource },
}

#[derive(Serialize, Clone)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Serialize, Clone)]
struct DocumentSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Deserialize)]
struct CountTokensResponse {
    input_tokens: u64,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
    has_more: bool,
    #[serde(default)]
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct ModelInfo {
    id: String,
    display_name: String,
    max_input_tokens: u64,
    #[serde(default)]
    created_at: String,
}

// --- File handling ---

fn image_media_type(ext: &str) -> Option<&'static str> {
    match ext {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn build_content(path: &Path) -> Result<Content> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    if ext == "pdf" {
        let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        return Ok(Content::Blocks(vec![
            ContentBlock::Document {
                source: DocumentSource {
                    source_type: "base64".into(),
                    media_type: "application/pdf".into(),
                    data: encoded,
                },
            },
            ContentBlock::Text {
                text: format!("File: {filename}"),
            },
        ]));
    }

    if let Some(media_type) = image_media_type(&ext) {
        let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        return Ok(Content::Blocks(vec![
            ContentBlock::Image {
                source: ImageSource {
                    source_type: "base64".into(),
                    media_type: media_type.into(),
                    data: encoded,
                },
            },
            ContentBlock::Text {
                text: format!("File: {filename}"),
            },
        ]));
    }

    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Content::Text(text))
}

async fn count_tokens(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    path: &Path,
) -> Result<u64> {
    let content = build_content(path)?;
    count_tokens_for_content(client, api_key, model, system, content).await
}

async fn count_tokens_for_content(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    content: Content,
) -> Result<u64> {
    let request = CountTokensRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".into(),
            content,
        }],
        system: system.map(String::from),
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages/count_tokens")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request)
        .send()
        .await
        .context("sending request to Anthropic API")?;

    if !response.status().is_success() {
        let status = response.status();
        if let Ok(err) = response.json::<ApiError>().await {
            bail!("API error ({}): {}", status, err.error.message);
        }
        bail!("API error: {}", status);
    }

    let result: CountTokensResponse = response.json().await.context("parsing API response")?;
    Ok(result.input_tokens)
}

async fn run_count(args: CountArgs) -> Result<()> {
    if args.files.is_empty() {
        bail!("No files specified. Usage: ctc <files...>\n\nRun `ctc --help` for more info.");
    }

    let api_key = load_api_key()?;
    let model = args.model.unwrap_or_else(load_default_model);
    let client = reqwest::Client::new();
    let mut results: Vec<FileResult> = Vec::new();
    let mut has_error = false;

    for path in &args.files {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        match count_tokens(&client, &api_key, &model, args.system.as_deref(), path).await {
            Ok(tokens) => {
                results.push(FileResult {
                    file: filename,
                    tokens,
                });
            }
            Err(e) => {
                eprintln!("{filename}: error - {e}");
                has_error = true;
            }
        }
    }

    let total: u64 = results.iter().map(|r| r.tokens).sum();

    match args.output {
        OutputFormat::Text => {
            for r in &results {
                println!("{}: {} tokens", r.file, r.tokens);
            }
            if results.len() > 1 {
                println!("\nTotal: {total} tokens");
            }
        }
        OutputFormat::Json => {
            let output = JsonOutput {
                model: model.clone(),
                files: results,
                total_tokens: total,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Csv => {
            println!("file,tokens");
            for r in &results {
                println!("{},{}", r.file, r.tokens);
            }
            if results.len() > 1 {
                println!("total,{total}");
            }
        }
    }

    if has_error {
        std::process::exit(1);
    }

    Ok(())
}

/// Pick the newest model of each tier (opus, sonnet, haiku) from the fetched list.
fn pick_default_models(models: &[ModelInfo]) -> Vec<String> {
    let mut sorted: Vec<&ModelInfo> = models.iter().collect();
    // created_at is RFC3339 — lexicographic sort gives newest first.
    // On a tie, prefer the shorter id so the clean alias (e.g. claude-haiku-4-5)
    // wins over a dated snapshot (claude-haiku-4-5-20251001).
    sorted.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then(a.id.len().cmp(&b.id.len()))
    });

    let mut picked = Vec::new();
    for tier in ["opus", "sonnet", "haiku"] {
        if let Some(m) = sorted.iter().find(|m| m.id.contains(tier)) {
            picked.push(m.id.clone());
        }
    }

    // Fallback: if no tier matched, take the newest few models as-is.
    if picked.is_empty() {
        picked = sorted.iter().take(3).map(|m| m.id.clone()).collect();
    }

    picked
}

/// Result of counting one (file, model) cell.
enum Cell {
    Ok(u64),
    Err,
}

impl Cell {
    fn tokens(&self) -> Option<u64> {
        match self {
            Cell::Ok(t) => Some(*t),
            Cell::Err => None,
        }
    }

    fn display(&self) -> String {
        match self {
            Cell::Ok(t) => t.to_string(),
            Cell::Err => "error".to_string(),
        }
    }
}

#[derive(Serialize)]
struct CompareFileJson {
    file: String,
    /// Token counts parallel to the `models` list; null where counting failed.
    tokens: Vec<Option<u64>>,
}

#[derive(Serialize)]
struct CompareJsonOutput {
    models: Vec<String>,
    files: Vec<CompareFileJson>,
    /// Per-model totals across all files (failed cells excluded).
    totals: Vec<u64>,
}

async fn run_compare(args: CompareArgs) -> Result<()> {
    if args.files.is_empty() {
        bail!("No files specified. Usage: ctc compare <files...>\n\nRun `ctc compare --help` for more info.");
    }

    let api_key = load_api_key()?;
    let client = reqwest::Client::new();

    let models: Vec<String> = match args.models {
        Some(m) if !m.is_empty() => m,
        _ => {
            eprintln!("Fetching models to pick the latest of each tier...");
            let fetched = fetch_models(&client, &api_key).await?;
            let picked = pick_default_models(&fetched);
            if picked.is_empty() {
                bail!("Could not determine default models. Specify them with -m model1,model2.");
            }
            picked
        }
    };

    // filename -> one Cell per model (parallel to `models`).
    let mut rows: Vec<(String, Vec<Cell>)> = Vec::new();
    let mut has_error = false;

    for path in &args.files {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let content = match build_content(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{filename}: error - {e}");
                has_error = true;
                rows.push((filename, models.iter().map(|_| Cell::Err).collect()));
                continue;
            }
        };

        let mut cells = Vec::with_capacity(models.len());
        for model in &models {
            match count_tokens_for_content(
                &client,
                &api_key,
                model,
                args.system.as_deref(),
                content.clone(),
            )
            .await
            {
                Ok(tokens) => cells.push(Cell::Ok(tokens)),
                Err(e) => {
                    eprintln!("{filename} @ {model}: error - {e}");
                    has_error = true;
                    cells.push(Cell::Err);
                }
            }
        }
        rows.push((filename, cells));
    }

    // Per-model totals across files (failed cells excluded).
    let totals: Vec<u64> = (0..models.len())
        .map(|i| rows.iter().filter_map(|(_, c)| c[i].tokens()).sum())
        .collect();

    match args.output {
        OutputFormat::Text => print_compare_text(&models, &rows, &totals),
        OutputFormat::Json => {
            let output = CompareJsonOutput {
                models: models.clone(),
                files: rows
                    .iter()
                    .map(|(file, cells)| CompareFileJson {
                        file: file.clone(),
                        tokens: cells.iter().map(|c| c.tokens()).collect(),
                    })
                    .collect(),
                totals,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Csv => {
            println!("file,{}", models.join(","));
            for (file, cells) in &rows {
                let vals: Vec<String> = cells.iter().map(|c| c.display()).collect();
                println!("{file},{}", vals.join(","));
            }
            if rows.len() > 1 {
                let vals: Vec<String> = totals.iter().map(|t| t.to_string()).collect();
                println!("total,{}", vals.join(","));
            }
        }
    }

    if has_error {
        std::process::exit(1);
    }

    Ok(())
}

fn print_compare_text(models: &[String], rows: &[(String, Vec<Cell>)], totals: &[u64]) {
    let show_total = rows.len() > 1;

    // Column widths: first column for filenames, one per model.
    let name_w = rows
        .iter()
        .map(|(f, _)| f.len())
        .chain(std::iter::once("File".len()))
        .chain(if show_total {
            Some("TOTAL".len())
        } else {
            None
        })
        .max()
        .unwrap_or(4);

    let col_w: Vec<usize> = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let cells_max = rows.iter().map(|(_, c)| c[i].display().len()).max().unwrap_or(0);
            let total_len = if show_total {
                totals[i].to_string().len()
            } else {
                0
            };
            m.len().max(cells_max).max(total_len)
        })
        .collect();

    // Header.
    print!("{:<name_w$}", "File");
    for (i, m) in models.iter().enumerate() {
        print!("  {:>w$}", m, w = col_w[i]);
    }
    println!();

    // File rows.
    for (file, cells) in rows {
        print!("{:<name_w$}", file);
        for (i, cell) in cells.iter().enumerate() {
            print!("  {:>w$}", cell.display(), w = col_w[i]);
        }
        println!();
    }

    // Total row.
    if show_total {
        let total_line_w = name_w + col_w.iter().map(|w| w + 2).sum::<usize>();
        println!("{}", "─".repeat(total_line_w));
        print!("{:<name_w$}", "TOTAL");
        for (i, t) in totals.iter().enumerate() {
            print!("  {:>w$}", t, w = col_w[i]);
        }
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => run_init(),
        Some(Commands::Model) => run_model().await,
        Some(Commands::Compare(args)) => run_compare(args).await,
        None => run_count(cli.count).await,
    }
}
