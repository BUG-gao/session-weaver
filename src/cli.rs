use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

use crate::catalog::{discover_sessions, register_codex_thread};
use crate::connectors::claude;
use crate::core::{ClientKind, Severity, validate_conversation};
use crate::pipeline::{
    MoveOptions, append_native_index, export_package, import_package, move_session, output_path,
    parse, render, write_jsonl_atomic,
};
use crate::portable::PortableEnvelope;

#[derive(Debug, Parser)]
#[command(
    name = "session-weaver",
    version,
    about = "Session Weaver - migrate Claude Code and Codex sessions safely"
)]
struct RootArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Scan(ScanArgs),
    Ls(BriefScanArgs),
    Inspect(ReadArgs),
    Show(BriefReadArgs),
    Check(ReadArgs),
    Ok(BriefReadArgs),
    Move(MoveArgs),
    #[command(name = "to-claude", visible_alias = "tc")]
    ToClaude(QuickMoveArgs),
    #[command(name = "to-codex", visible_alias = "tx")]
    ToCodex(QuickMoveArgs),
    Export(ExportArgs),
    Pack(BriefExportArgs),
    Import(ImportArgs),
    Unpack(BriefImportArgs),
    Doctor(DoctorArgs),
    Env(DoctorArgs),
}

#[derive(Debug, Args)]
struct ScanArgs {
    #[arg(long, short)]
    client: ClientKind,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct BriefScanArgs {
    client: ClientKind,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ReadArgs {
    input: PathBuf,
    #[arg(long = "from", short = 'f')]
    source: ClientKind,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct BriefReadArgs {
    client: ClientKind,
    input: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct MoveArgs {
    input: PathBuf,
    #[arg(long = "from", short = 's')]
    source: ClientKind,
    #[arg(long = "to", short = 't')]
    target: ClientKind,
    #[arg(long, short)]
    output: Option<PathBuf>,
    #[arg(long, short = 'f')]
    overwrite: bool,
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    claude_model: String,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct QuickMoveArgs {
    input: PathBuf,
    #[arg(long, short)]
    output: Option<PathBuf>,
    #[arg(long, short = 'f')]
    overwrite: bool,
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    claude_model: String,
    #[arg(long)]
    open: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ExportArgs {
    input: PathBuf,
    output: PathBuf,
    #[arg(long = "from", short = 'f')]
    source: ClientKind,
}

#[derive(Debug, Args)]
struct BriefExportArgs {
    client: ClientKind,
    input: PathBuf,
    output: PathBuf,
}

#[derive(Debug, Args)]
struct ImportArgs {
    input: PathBuf,
    #[arg(long = "to", short = 't')]
    target: ClientKind,
    #[arg(long, short)]
    output: Option<PathBuf>,
    #[arg(long, short = 'f')]
    overwrite: bool,
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    claude_model: String,
}

#[derive(Debug, Args)]
struct BriefImportArgs {
    target: ClientKind,
    input: PathBuf,
    #[arg(long, short)]
    output: Option<PathBuf>,
    #[arg(long, short = 'f')]
    overwrite: bool,
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    claude_model: String,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    #[arg(long)]
    json: bool,
}

pub fn run() -> Result<()> {
    match RootArgs::parse().command {
        Command::Scan(args) => scan(args),
        Command::Ls(args) => scan(ScanArgs {
            client: args.client,
            root: args.root,
            json: args.json,
        }),
        Command::Inspect(args) => inspect(args),
        Command::Show(args) => inspect(ReadArgs {
            input: args.input,
            source: args.client,
            json: args.json,
        }),
        Command::Check(args) => check(args),
        Command::Ok(args) => check(ReadArgs {
            input: args.input,
            source: args.client,
            json: args.json,
        }),
        Command::Move(args) => move_command(args),
        Command::ToClaude(args) => quick_move_command(ClientKind::Codex, ClientKind::Claude, args),
        Command::ToCodex(args) => quick_move_command(ClientKind::Claude, ClientKind::Codex, args),
        Command::Export(args) => export(args),
        Command::Pack(args) => export(ExportArgs {
            input: args.input,
            output: args.output,
            source: args.client,
        }),
        Command::Import(args) => import(args),
        Command::Unpack(args) => import(ImportArgs {
            input: args.input,
            target: args.target,
            output: args.output,
            overwrite: args.overwrite,
            claude_model: args.claude_model,
        }),
        Command::Doctor(args) => doctor(args),
        Command::Env(args) => doctor(args),
    }
}

fn scan(args: ScanArgs) -> Result<()> {
    let root = match args.root {
        Some(root) => root,
        None => default_root(args.client)?,
    };
    let sessions = discover_sessions(args.client, &root)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
    } else {
        for session in sessions {
            println!("{}\t{}", session.id, session.path.display());
        }
    }
    Ok(())
}

fn inspect(args: ReadArgs) -> Result<()> {
    let input = resolve_source(&args.input, args.source)?;
    let conversation = parse(args.source, &input)?;
    let summary = json!({
        "client": args.source,
        "session_id": conversation.id,
        "title": conversation.metadata.title,
        "cwd": conversation.metadata.cwd,
        "entries": conversation.entries.len(),
    });
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("客户端: {}", args.source);
        println!(
            "会话 ID: {}",
            summary["session_id"].as_str().unwrap_or_default()
        );
        println!("事件数: {}", conversation.entries.len());
    }
    Ok(())
}

fn check(args: ReadArgs) -> Result<()> {
    let input = resolve_source(&args.input, args.source)?;
    let conversation = parse(args.source, &input)?;
    let mut findings = validate_conversation(&conversation);
    if args.source == ClientKind::Claude {
        findings.extend(claude::validate_native(&read_jsonl(&input)?));
    }
    let failed = findings
        .iter()
        .any(|finding| finding.severity == Severity::Error);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&findings)?);
    } else if findings.is_empty() {
        println!("兼容性检查通过: {} 个事件", conversation.entries.len());
    } else {
        for finding in &findings {
            println!(
                "{:?} {} record={:?} path={:?}: {}",
                finding.severity, finding.code, finding.record, finding.path, finding.message
            );
        }
    }
    if failed {
        bail!("compatibility check failed");
    }
    Ok(())
}

fn move_command(args: MoveArgs) -> Result<()> {
    if args.source == args.target {
        bail!("source and target clients must differ");
    }
    let input = resolve_source(&args.input, args.source)?;
    let root = match args.output {
        Some(root) => root,
        None => default_root(args.target)?,
    };
    let report = move_session(MoveOptions {
        source: &input,
        source_kind: args.source,
        target_kind: args.target,
        target_root: &root,
        overwrite: args.overwrite,
        claude_model: &args.claude_model,
    })?;
    if args.target == ClientKind::Codex {
        let conversation = parse(args.target, &report.output)?;
        register_codex_thread(&root, &conversation, &report.output)?;
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("迁移完成: {} -> {}", args.source, args.target);
        println!("新会话 ID: {}", report.target_id);
        println!("保存位置: {}", report.output.display());
        println!(
            "恢复命令: {}",
            resume_command(args.target, &report.target_id)
        );
    }
    if args.open {
        open_client(args.target, &report.target_id, &root)?;
    }
    Ok(())
}

fn quick_move_command(source: ClientKind, target: ClientKind, args: QuickMoveArgs) -> Result<()> {
    move_command(MoveArgs {
        input: args.input,
        source,
        target,
        output: args.output,
        overwrite: args.overwrite,
        claude_model: args.claude_model,
        open: args.open,
        json: args.json,
    })
}

fn export(args: ExportArgs) -> Result<()> {
    let input = resolve_source(&args.input, args.source)?;
    let conversation = parse(args.source, &input)?;
    let package = export_package(&conversation)?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, serde_json::to_vec_pretty(&package)?)?;
    println!("{}", args.output.display());
    Ok(())
}

fn import(args: ImportArgs) -> Result<()> {
    let package: PortableEnvelope = serde_json::from_slice(&fs::read(&args.input)?)?;
    let mut conversation = import_package(&package)?;
    conversation.id = match args.target {
        ClientKind::Claude => uuid::Uuid::new_v4().to_string(),
        ClientKind::Codex => uuid::Uuid::now_v7().to_string(),
    };
    let root = match args.output {
        Some(root) => root,
        None => default_root(args.target)?,
    };
    let values = render(args.target, &conversation, &args.claude_model)?;
    let path = output_path(args.target, &root, &conversation);
    write_jsonl_atomic(&path, &values, args.overwrite)?;
    parse(args.target, &path).context("import read-back validation failed")?;
    append_native_index(args.target, &root, &conversation)?;
    if args.target == ClientKind::Codex {
        register_codex_thread(&root, &conversation, &path)?;
    }
    println!("{}", path.display());
    Ok(())
}

fn doctor(args: DoctorArgs) -> Result<()> {
    let report = json!({
        "rust_version": command_version("rustc", &["--version"]),
        "claude_version": command_version("claude", &["--version"]),
        "codex_version": command_version("codex", &["--version"]),
        "claude_root": default_root(ClientKind::Claude)?.display().to_string(),
        "codex_root": default_root(ClientKind::Codex)?.display().to_string(),
    });
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Rust: {}", report["rust_version"]);
        println!("Claude Code: {}", report["claude_version"]);
        println!("Codex: {}", report["codex_version"]);
    }
    Ok(())
}

fn default_root(client: ClientKind) -> Result<PathBuf> {
    let primary = match client {
        ClientKind::Claude => [
            "SESSION_WEAVER_CLAUDE_HOME",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_HOME",
        ],
        ClientKind::Codex => ["SESSION_WEAVER_CODEX_HOME", "CODEX_HOME", "CODEX_HOME"],
    };
    for name in primary {
        if let Some(value) = std::env::var_os(name) {
            return Ok(PathBuf::from(value));
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("neither HOME nor USERPROFILE is set")?;
    Ok(PathBuf::from(home).join(match client {
        ClientKind::Claude => ".claude",
        ClientKind::Codex => ".codex",
    }))
}

fn resolve_source(input: &Path, client: ClientKind) -> Result<PathBuf> {
    if input.exists() {
        return Ok(input.to_path_buf());
    }
    let id = input
        .to_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("input must be a path or session id")?;
    let matches = discover_sessions(client, &default_root(client)?)?
        .into_iter()
        .filter(|session| session.id == id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [session] => Ok(session.path.clone()),
        [] => bail!("{client} session not found: {id}"),
        _ => bail!("{client} session id is ambiguous: {id}"),
    }
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>> {
    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

fn command_version(program: &str, args: &[&str]) -> Option<String> {
    ProcessCommand::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_string())
}

fn resume_command(client: ClientKind, id: &str) -> String {
    match client {
        ClientKind::Claude => format!("claude --resume {id}"),
        ClientKind::Codex => format!("codex resume {id}"),
    }
}

fn open_client(client: ClientKind, id: &str, root: &Path) -> Result<()> {
    let mut command = match client {
        ClientKind::Claude => {
            let mut command = ProcessCommand::new("claude");
            command.arg("--resume").arg(id);
            command.env("CLAUDE_CONFIG_DIR", root);
            command
        }
        ClientKind::Codex => {
            let mut command = ProcessCommand::new("codex");
            command.arg("resume").arg(id);
            command.env("CODEX_HOME", root);
            command
        }
    };
    let status = command.status()?;
    if !status.success() {
        bail!("{client} exited with {status}");
    }
    Ok(())
}
