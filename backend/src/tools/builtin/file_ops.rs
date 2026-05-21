use crate::tools::builtin::{glob_match, resolve_path};
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册文件操作相关工具: read_file, write_file, edit_file, rename_file, glob, grep, list_dir, search_replace
pub async fn register(registry: &ToolRegistry) {
    // read_file tool
    registry
        .register(ToolDef {
            name: "read_file".to_string(),
            description:
                "Read file content. Params: path (required), offset (optional line number), limit (optional max lines)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Starting line number"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read"
                    }
                },
                "required": ["path"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let resolved_path = resolve_path(path, &args);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;

                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

                    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
                    Ok(lines.join("\n"))
                },
            ),
        })
        .await;

    // write_file tool
    registry
        .register(ToolDef {
            name: "write_file".to_string(),
            description:
                "Write content to a file (auto-creates dirs). Params: path (required), content (required)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let content = args["content"].as_str().ok_or("Missing 'content' parameter")?;

                    let resolved_path = resolve_path(path, &args);

                    if let Some(parent) = resolved_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create directory: {}", e))?;
                    }
                    std::fs::write(&resolved_path, content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    Ok(format!("Successfully wrote file: {}", resolved_path.display()))
                },
            ),
        })
        .await;

    // edit_file tool
    registry
        .register(ToolDef {
            name: "edit_file".to_string(),
            description:
                "Find and replace text in a file (1 replacement). Params: path (required), old_string (required), new_string (required)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Text to search for and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "New text to replace with"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let old_str = args["old_string"].as_str().ok_or("Missing 'old_string' parameter")?;
                    let new_str = args["new_string"].as_str().ok_or("Missing 'new_string' parameter")?;

                    let resolved_path = resolve_path(path, &args);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;

                    if !content.contains(old_str) {
                        return Err(
                            "Text not found: the 'old_string' does not exist in the file. Make sure to match exact content including whitespace."
                                .to_string(),
                        );
                    }

                    // 只替换第一次出现
                    let new_content = content.replacen(old_str, new_str, 1);
                    std::fs::write(&resolved_path, &new_content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    Ok(format!("Successfully edited file: {}", resolved_path.display()))
                },
            ),
        })
        .await;

    // glob file search tool
    registry
        .register(ToolDef {
            name: "glob".to_string(),
            description:
                "Search files by glob pattern. Params: pattern (required, e.g. **/*.rs or **/*), path (optional directory)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern, e.g. **/*.rs"
                    },
                    "path": {
                        "type": "string",
                        "description": "Root directory to search (relative paths resolve to workspace, absolute paths used directly); defaults to workspace"
                    }
                },
                "required": ["pattern"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let pattern =
                        args["pattern"].as_str().ok_or("Missing 'pattern' parameter - provide a glob pattern like **/*.rs")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    let base = match base {
                        "" | "." | "workspace" | "workspace/" | "workspace\\" => ".",
                        other => other,
                    };

                    let resolved_base = resolve_path(base, &args);

                    let glob_pattern = if resolved_base.to_string_lossy().ends_with('/')
                        || resolved_base.to_string_lossy().ends_with('\\')
                    {
                        format!("{}{}", resolved_base.display(), pattern)
                    } else {
                        format!("{}/{}", resolved_base.display(), pattern)
                    };

                    match glob::glob(&glob_pattern) {
                        Ok(entries) => {
                            let mut results = Vec::new();
                            for entry in entries.flatten() {
                                results.push(entry.display().to_string());
                                if results.len() >= 200 {
                                    results.push("...(results truncated)".to_string());
                                    break;
                                }
                            }
                            if results.is_empty() {
                                Ok("No files matched the pattern".to_string())
                            } else {
                                Ok(results.join("\n"))
                            }
                        }
                        Err(e) => Err(format!("Invalid glob pattern: {}", e)),
                    }
                },
            ),
        })
        .await;

    // grep content search tool
    registry
        .register(ToolDef {
            name: "grep".to_string(),
            description:
                "Search text in files with regex. Pass a directory path as 'path' (not a glob pattern). Params: pattern (required), path (optional directory, e.g. '.' or 'scripts'), include (optional file filter like '*.rs')"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search (relative paths resolve to workspace, absolute paths used directly); defaults to workspace"
                    },
                    "include": {
                        "type": "string",
                        "description": "File filter pattern, e.g. *.rs"
                    }
                },
                "required": ["pattern"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let pattern =
                        args["pattern"].as_str().ok_or("Missing 'pattern' parameter - provide a regex pattern to search for")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    let base = match base
                        .trim_end_matches('*')
                        .trim_end_matches('/')
                        .trim_end_matches('\\')
                    {
                        "" | "." | "workspace" | "workspace/" | "workspace\\" => ".",
                        other => other,
                    };
                    let include = args["include"].as_str();

                    let resolved_base = resolve_path(base, &args);

                    let re = regex::RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .multi_line(true)
                        .build()
                        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

                    let mut results = Vec::new();
                    let walker = walkdir::WalkDir::new(resolved_base).max_depth(20);

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_dir() {
                            continue;
                        }

                        if let Some(ref inc) = include {
                            if let Some(name) = entry.file_name().to_str() {
                                if !glob_match(name, inc) {
                                    continue;
                                }
                            }
                        }

                        if let Ok(file) = std::fs::File::open(entry.path()) {
                            let mut reader = std::io::BufReader::new(file);
                            let mut line_no = 0usize;
                            loop {
                                let mut line = String::new();
                                match std::io::BufRead::read_line(&mut reader, &mut line) {
                                    Ok(0) => break,
                                    Ok(_) => {
                                        line_no += 1;
                                        let trimmed = line.trim_end();
                                        if re.is_match(trimmed) {
                                            results.push(format!(
                                                "{}:{}: {}",
                                                entry.path().display(),
                                                line_no,
                                                trimmed
                                            ));
                                            if results.len() >= 100 {
                                                break;
                                            }
                                        }
                                    }
                                    Err(_) => continue,
                                }
                            }
                        }
                        if results.len() >= 100 {
                            break;
                        }
                    }

                    if results.is_empty() {
                        Ok("No matches found - try a different search term, pattern, or file path".to_string())
                    } else {
                        Ok(results.join("\n"))
                    }
                },
            ),
        })
        .await;

    // search_replace tool - batch find and replace across files
    registry
        .register(ToolDef {
            name: "search_replace".to_string(),
            description:
                "Batch find and replace text across multiple files using regex. Params: pattern (required, regex), replacement (required), path (optional directory, default workspace), include (optional file filter like '*.rs' or '*.tsx')"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "replacement": {
                        "type": "string",
                        "description": "Replacement text"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (defaults to workspace root)"
                    },
                    "include": {
                        "type": "string",
                        "description": "File filter, e.g. '*.rs' or '*.tsx'"
                    }
                },
                "required": ["pattern", "replacement"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern' parameter")?;
                    let replacement =
                        args["replacement"].as_str().ok_or("Missing 'replacement' parameter")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    let base = match base
                        .trim_end_matches('*')
                        .trim_end_matches('/')
                        .trim_end_matches('\\')
                    {
                        "" | "." | "workspace" => ".",
                        other => other,
                    };
                    let include = args["include"].as_str();

                    let resolved_base = resolve_path(base, &args);
                    let re = regex::RegexBuilder::new(pattern)
                        .multi_line(true)
                        .build()
                        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

                    let walker = walkdir::WalkDir::new(&resolved_base).max_depth(20);
                    let mut total_changes = 0usize;
                    let mut changed_files: Vec<String> = Vec::new();

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_dir() {
                            continue;
                        }
                        if let Some(ref inc) = include {
                            if let Some(name) = entry.file_name().to_str() {
                                if !glob_match(name, inc) {
                                    continue;
                                }
                            }
                        }
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if re.is_match(&content) {
                                let new_content = re.replace_all(&content, replacement);
                                if new_content != content {
                                    if std::fs::write(entry.path(), new_content.as_ref()).is_ok() {
                                        total_changes += 1;
                                        changed_files.push(entry.path().display().to_string());
                                    }
                                }
                            }
                        }
                    }

                    if changed_files.is_empty() {
                        Ok("No matches found - no files were modified".to_string())
                    } else {
                        Ok(format!(
                            "Replaced matches in {} file(s):\n{}",
                            total_changes,
                            changed_files.join("\n")
                        ))
                    }
                },
            ),
        })
        .await;

    // list_dir tool - list directory contents
    registry
        .register(ToolDef {
            name: "list_dir".to_string(),
            description:
                "List files and directories. Params: path (optional, default workspace). Returns directory entries with name, type, and size"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path (relative to workspace or absolute)"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Max recursion depth (default 1, use 0 for unlimited)"
                    }
                },
                "required": []
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let base = args["path"].as_str().unwrap_or(".");
                    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                    let resolved_base = resolve_path(base, &args);

                    if !resolved_base.exists() {
                        return Err(format!("Directory not found: {}", resolved_base.display()));
                    }
                    if !resolved_base.is_dir() {
                        return Err(format!("Not a directory: {}", resolved_base.display()));
                    }

                    let max_depth = if depth == 0 { usize::MAX } else { depth };
                    let walker = walkdir::WalkDir::new(&resolved_base).max_depth(max_depth);
                    let mut entries: Vec<String> = Vec::new();

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        let indent = entry.depth().saturating_sub(1);
                        let prefix = "  ".repeat(indent);
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') || name == "node_modules" {
                            continue;
                        }
                        let file_type = if entry.file_type().is_dir() { "📁" } else { "📄" };
                        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                        let size_str = if size > 1024 * 1024 {
                            format!(" ({:.1} MB)", size as f64 / 1024.0 / 1024.0)
                        } else if size > 1024 {
                            format!(" ({:.1} KB)", size as f64 / 1024.0)
                        } else if size > 0 {
                            format!(" ({} B)", size)
                        } else {
                            String::new()
                        };
                        entries.push(format!("{}{} {}{}", prefix, file_type, name, size_str));
                    }

                    if entries.is_empty() {
                        Ok("(empty directory)".to_string())
                    } else {
                        Ok(entries.join("\n"))
                    }
                },
            ),
        })
        .await;

    // rename_file tool
    registry
        .register(ToolDef {
            name: "rename_file".to_string(),
            description:
                "Rename or move a file/directory. Params: path (required, source), new_path (required, destination)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Source file or directory path"
                    },
                    "new_path": {
                        "type": "string",
                        "description": "Destination file or directory path"
                    }
                },
                "required": ["path", "new_path"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let old_path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let new_path = args["new_path"].as_str().ok_or("Missing 'new_path' parameter")?;
                    let old_resolved = resolve_path(old_path, &args);
                    let new_resolved = resolve_path(new_path, &args);

                    if !old_resolved.exists() {
                        return Err(format!("Source not found: {}", old_resolved.display()));
                    }
                    if new_resolved.exists() {
                        return Err(format!("Destination already exists: {}", new_resolved.display()));
                    }

                    if let Some(parent) = new_resolved.parent() {
                        if !parent.exists() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| format!("Failed to create destination directory: {}", e))?;
                        }
                    }

                    std::fs::rename(&old_resolved, &new_resolved)
                        .map_err(|e| format!("Failed to rename: {}", e))?;

                    Ok(format!("Renamed: {} -> {}", old_resolved.display(), new_resolved.display()))
                },
            ),
        })
        .await;
}
