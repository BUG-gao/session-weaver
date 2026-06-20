//! Registration with the Claude desktop app's session store.
//!
//! The Claude desktop app does **not** populate its history list by scanning
//! `~/.claude/projects/*.jsonl`. It keeps a separate per-session registry at
//!
//! ```text
//! <app-data>/claude-code-sessions/<account-id>/<workspace-id>/local_<cliSessionId>.json
//! ```
//!
//! A migrated transcript that only lands in `~/.claude/projects` is therefore
//! invisible in the app until a matching registry file exists. We cannot
//! derive the app-assigned `<account-id>/<workspace-id>` pair, so we reuse the
//! directory of an existing session that shares the same `cwd`. When no such
//! workspace exists (the app was never opened in that project, or is not
//! installed) registration is skipped and migration still succeeds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

/// The fields needed to describe a session in the desktop registry.
pub struct DesktopSession<'a> {
    pub cli_session_id: &'a str,
    pub cwd: &'a str,
    pub model: &'a str,
    pub title: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub created_ms: i64,
    pub last_activity_ms: i64,
    pub completed_turns: usize,
}

/// Resolve the `claude-code-sessions` base directory for the desktop app.
///
/// Honours `SESSION_WEAVER_CLAUDE_DESKTOP_HOME` (pointing at the
/// `claude-code-sessions` directory) for tests and non-standard installs,
/// then falls back to the per-platform default. Returns `None` when no
/// candidate directory exists.
pub fn sessions_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("SESSION_WEAVER_CLAUDE_DESKTOP_HOME") {
        let path = PathBuf::from(value);
        return path.exists().then_some(path);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = PathBuf::from(home);
    let candidates = [
        // macOS
        home.join("Library/Application Support/Claude/claude-code-sessions"),
        // Linux
        home.join(".config/Claude/claude-code-sessions"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

/// Register `session` with the desktop app by reusing the workspace directory
/// of an existing session that shares the same `cwd`.
///
/// Returns the written path on success, or `Ok(None)` when registration was
/// skipped because no matching workspace directory exists.
pub fn register_session(base: &Path, session: &DesktopSession<'_>) -> Result<Option<PathBuf>> {
    if !base.exists() {
        return Ok(None);
    }
    let Some(workspace) = find_workspace_dir(base, session.cwd)? else {
        return Ok(None);
    };
    let path = workspace.join(format!("local_{}.json", session.cli_session_id));
    let value = entry_value(session);
    let serialized = serde_json::to_vec_pretty(&value)?;
    fs::write(&path, serialized)?;
    Ok(Some(path))
}

/// Find the workspace directory (`<base>/<account>/<workspace>`) that already
/// holds a session for `cwd`, by reading existing `local_*.json` files.
fn find_workspace_dir(base: &Path, cwd: &str) -> Result<Option<PathBuf>> {
    for account in read_dirs(base)? {
        for workspace in read_dirs(&account)? {
            for file in fs::read_dir(&workspace)?.flatten() {
                let path = file.path();
                let is_registry = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("local_") && name.ends_with(".json"));
                if !is_registry {
                    continue;
                }
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if value.get("cwd").and_then(Value::as_str) == Some(cwd) {
                    return Ok(Some(workspace));
                }
            }
        }
    }
    Ok(None)
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    Ok(fs::read_dir(path)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect())
}

fn entry_value(session: &DesktopSession<'_>) -> Value {
    let written_branches: Vec<&str> = session
        .git_branch
        .filter(|branch| !branch.is_empty() && *branch != "HEAD")
        .into_iter()
        .collect();
    json!({
        "sessionId": format!("local_{}", session.cli_session_id),
        "cliSessionId": session.cli_session_id,
        "cwd": session.cwd,
        "originCwd": session.cwd,
        "createdAt": session.created_ms,
        "lastFocusedAt": session.last_activity_ms,
        "lastActivityAt": session.last_activity_ms,
        "model": session.model,
        "isArchived": false,
        "title": session.title.unwrap_or("Imported session (codex→claude)"),
        "titleSource": "auto",
        // Conservative permission posture: never pre-authorise tools or
        // directories for an imported session. The user grants access in-app.
        "permissionMode": "default",
        "remoteMcpServersConfig": [],
        "writtenBranches": written_branches,
        "completedTurns": session.completed_turns,
        "alwaysAllowedReasons": [],
        "sessionPermissionUpdates": [],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn registers_into_matching_workspace_only() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let workspace = base.join("acct-1").join("ws-1");
        fs::create_dir_all(&workspace).unwrap();
        // Existing session pins the workspace to this cwd.
        fs::write(
            workspace.join("local_existing.json"),
            json!({"cliSessionId": "existing", "cwd": "/tmp/project"}).to_string(),
        )
        .unwrap();

        let session = DesktopSession {
            cli_session_id: "new-id",
            cwd: "/tmp/project",
            model: "claude-opus-4-8",
            title: Some("demo"),
            git_branch: Some("main"),
            created_ms: 1,
            last_activity_ms: 2,
            completed_turns: 3,
        };
        let written = register_session(base, &session).unwrap().unwrap();
        assert_eq!(written, workspace.join("local_new-id.json"));

        let value: Value =
            serde_json::from_str(&fs::read_to_string(&written).unwrap()).unwrap();
        assert_eq!(value["cliSessionId"], "new-id");
        assert_eq!(value["permissionMode"], "default");
        assert!(value.get("chromePermissionMode").is_none());
        assert_eq!(value["writtenBranches"], json!(["main"]));
    }

    #[test]
    fn skips_when_no_workspace_matches_cwd() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let workspace = base.join("acct-1").join("ws-1");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(
            workspace.join("local_existing.json"),
            json!({"cliSessionId": "existing", "cwd": "/some/other/project"}).to_string(),
        )
        .unwrap();

        let session = DesktopSession {
            cli_session_id: "new-id",
            cwd: "/tmp/project",
            model: "claude-opus-4-8",
            title: None,
            git_branch: None,
            created_ms: 1,
            last_activity_ms: 2,
            completed_turns: 0,
        };
        assert!(register_session(base, &session).unwrap().is_none());
    }
}
