use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 apply_patch 工具（unified diff 应用）
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
            name: "apply_patch".to_string(),
            description: "Apply a unified diff (patch) to files. The diff format is standard 'diff -u' output with ---/+++ headers and @@ hunks. Params: diff (required, the patch content)"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "diff": {
                        "type": "string",
                        "description": "Unified diff content to apply"
                    }
                },
                "required": ["diff"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let diff = args["diff"].as_str().ok_or("Missing 'diff' parameter")?;
                    apply_unified_diff(diff, &args).map(|summary| {
                        format!("Patch applied successfully:\n{}", summary.join("\n"))
                    })
                },
            ),
        })
        .await;
}

/// Apply a unified diff patch to files
fn apply_unified_diff(diff: &str, _args: &serde_json::Value) -> Result<Vec<String>, String> {
    let mut summary: Vec<String> = Vec::new();
    let mut current_file: Option<String> = None;
    let mut hunks: Vec<(usize, Vec<String>)> = Vec::new(); // (start_line, hunk_lines)

    // Parse unified diff format
    for line in diff.lines() {
        if line.starts_with("--- ") {
            continue; // Skip original file header
        }
        if line.starts_with("+++ ") {
            // New file header -> finalize current file
            if let Some(ref file) = current_file {
                if !hunks.is_empty() {
                    apply_hunks_to_file(file, &hunks)?;
                    summary.push(format!("  Patched: {}", file));
                }
            }
            let path = &line[4..].trim();
            current_file = Some(path.to_string());
            hunks.clear();
            continue;
        }
        if line.starts_with("@@ ") {
            // Parse hunk header: @@ -start,count +start,count @@
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(new_start) = parts.get(1).and_then(|s| s.split(',').next()) {
                    let start_line = new_start.trim_start_matches('+').parse::<usize>().unwrap_or(1);
                    hunks.push((start_line, Vec::new()));
                }
            }
            continue;
        }

        if let Some((_, ref mut lines)) = hunks.last_mut() {
            lines.push(line.to_string());
        }
    }

    // Apply last file
    if let Some(ref file) = current_file {
        if !hunks.is_empty() {
            apply_hunks_to_file(file, &hunks)?;
            summary.push(format!("  Patched: {}", file));
        }
    }

    if summary.is_empty() {
        return Err("No patch hunks to apply - check diff format".to_string());
    }

    Ok(summary)
}

/// Apply parsed hunks to a single file
fn apply_hunks_to_file(file: &str, hunks: &[(usize, Vec<String>)]) -> Result<(), String> {
    let path = std::path::Path::new(file);
    let content = if path.exists() {
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read '{}': {}", file, e))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    // Apply hunks in reverse order to preserve line numbers
    for (hunk_start, hunk_lines) in hunks.iter().rev() {
        let mut insertions: Vec<(usize, String)> = Vec::new();
        let mut deletions: Vec<usize> = Vec::new();
        let mut current_line = hunk_start - 1; // 0-based

        for hunk_line in hunk_lines {
            if hunk_line.starts_with("+") {
                let text = &hunk_line[1..];
                insertions.push((current_line.min(lines.len()), text.to_string()));
            } else if hunk_line.starts_with("-") {
                if current_line < lines.len() {
                    deletions.push(current_line);
                }
            } else if hunk_line.starts_with(" ") {
                current_line = current_line.saturating_add(1);
            }
        }

        // Apply deletions (reverse order to preserve indices)
        for &dl in deletions.iter().rev() {
            if dl < lines.len() {
                lines.remove(dl);
            }
        }

        // Apply insertions
        let mut offset = 0i32;
        for (pos, text) in &insertions {
            let adj_pos = ((*pos as i32) + offset).max(0) as usize;
            if adj_pos <= lines.len() {
                lines.insert(adj_pos, text.clone());
                offset += 1;
            }
        }
    }

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    std::fs::write(path, lines.join("\n"))
        .map_err(|e| format!("Failed to write '{}': {}", file, e))?;

    Ok(())
}
