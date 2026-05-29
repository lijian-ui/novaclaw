use crate::tools::builtin::{glob_match, resolve_path};
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

/// 格式化文件大小为可读字符串
fn format_file_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// ── read_file 常量 ──
/// 全量读取阈值：小于此值的文件直接全量返回
const FULL_READ_THRESHOLD_BYTES: u64 = 64 * 1024; // 64KB
/// 单次读取最大行数
const MAX_LINES: usize = 2000;
/// 单行最长字符数（超长行截断）
const MAX_LINE_LENGTH: usize = 2000;
/// 输出字节硬上限
const OUTPUT_MAX_BYTES: usize = 50 * 1024; // 50KB
/// 二进制检测的采样大小
const BINARY_SAMPLE_BYTES: usize = 4096;

/// 重复读取计数器（path → 连续读取次数）
static READ_REPEAT_COUNTER: once_cell::sync::Lazy<Mutex<HashMap<String, usize>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

/// 重置指定路径的重复读取计数器（由 write_file/edit_file 等工具调用）
pub fn reset_read_repeat_counter(path: &str) {
    if let Ok(mut counter) = READ_REPEAT_COUNTER.lock() {
        counter.remove(path);
    }
}

/// 注册文件操作相关工具: read_file, write_file, edit_file, rename_file, glob, grep, list_dir, search_replace
pub async fn register(registry: &ToolRegistry) {
    // read_file tool
    registry
        .register(ToolDef {
                        name: "read_file".to_string(),
            display_name: "读取文件".to_string(),
            description:
                "Read file content with line numbers and pagination. \
                 Params: path (required), offset (optional line number, default 1), \
                 limit (optional max lines, default 2000, max 2000), \
                 head (optional, first N lines), tail (optional, last N lines), \
                 range (optional, inclusive range like 'A-B', 1-indexed). \
                 Files over 64KB auto-switch to outline mode (metadata + first 80 lines + symbol list). \
                 Use offset/limit, head/tail, or range to read specific sections of large files. \
                 Lines longer than 2000 chars are truncated. Output capped at 50KB."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to working directory, absolute paths used directly)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start from (1-indexed, default 1)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to read (default 2000, max 2000)"
                    },
                    "head": {
                        "type": "integer",
                        "description": "If set, return only the first N lines"
                    },
                    "tail": {
                        "type": "integer",
                        "description": "If set, return only the last N lines"
                    },
                    "range": {
                        "type": "string",
                        "description": "Inclusive line range like '50-100' or '50-50'. 1-indexed. Takes precedence over head/tail."
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
                    let path_str = resolved_path.to_string_lossy().to_string();

                    // 检查文件是否存在 + 获取元数据
                    let metadata = std::fs::metadata(&resolved_path)
                        .map_err(|e| format!("Failed to access file: {}", e))?;
                    let file_size = metadata.len();
                    let _mtime = metadata.modified().ok();

                    // ── 二进制检测：采样前 4KB 检查 NUL 字节 ──
                    if file_size > 0 {
                        use std::io::Read;
                        let mut file = std::fs::File::open(&resolved_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;
                        let mut sample = vec![0u8; BINARY_SAMPLE_BYTES.min(file_size as usize)];
                        let n = file.read(&mut sample).unwrap_or(0);
                        if sample[..n].contains(&0u8) {
                            return Ok(format!(
                                "<path>{}</path>\n<type>file</type>\n<binary>true</binary>\n\n\
                                 [Refused: file appears to be binary. Use 'file' tool or check file info instead.]",
                                path_str
                            ));
                        }
                    }

                    if metadata.is_dir() {
                        return Err(format!("'{}' is a directory, not a file", path_str));
                    }

                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;
                    let total_lines = content.lines().count();

                    // ── 确定读取范围 ──
                    // range > head/tail > offset/limit > default(full)
                    let (start_line, read_limit): (usize, usize) = {
                        if let Some(range_str) = args.get("range").and_then(|v| v.as_str()) {
                            // 解析 "A-B" 格式
                            if let Some((a, b)) = range_str.split_once('-') {
                                let s = a.trim().parse::<usize>().unwrap_or(1).max(1);
                                let e = b.trim().parse::<usize>().unwrap_or(total_lines).max(s);
                                (s - 1, (e - s + 1).min(MAX_LINES))
                            } else {
                                (0, MAX_LINES)
                            }
                        } else if let Some(head) = args.get("head").and_then(|v| v.as_u64()) {
                            (0, (head as usize).min(MAX_LINES))
                        } else if let Some(tail) = args.get("tail").and_then(|v| v.as_u64()) {
                            let t = (tail as usize).min(MAX_LINES);
                            if t >= total_lines { (0, total_lines) }
                            else { (total_lines - t, total_lines) }
                        } else {
                            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
                            (offset.saturating_sub(1), limit.min(MAX_LINES))
                        }
                    };

                    // ── 重复读取检测 ──
                    let repeat_key = format!("{}:{}:{}", path_str, start_line, read_limit);
                    {
                        if let Ok(mut counter) = READ_REPEAT_COUNTER.lock() {
                            let count = counter.entry(repeat_key.clone()).or_insert(0);
                            *count += 1;
                            if *count >= 4 {
                                return Ok(format!(
                                    "<path>{}</path>\n<type>file</type>\n<status>blocked</status>\n\n\
                                     [BLOCKED: You have read this exact file region 4 times in a row. \
                                     The content has NOT changed. You already have this information. \
                                     STOP re-reading and proceed with your task.]",
                                    path_str
                                ));
                            } else if *count >= 3 {
                                // 第3次附加警告
                            }
                        }
                    }

                    // ── 大文件检测：如果没有分片参数且文件 > 64KB，提示用分片读取 ──
                    let no_paging = args.get("range").is_none()
                        && args.get("head").is_none()
                        && args.get("tail").is_none();
                    if no_paging && file_size > FULL_READ_THRESHOLD_BYTES && start_line == 0 && read_limit >= MAX_LINES {
                        // 返回大纲模式：前 80 行 + 文件信息
                        let outline_limit = 80usize.min(total_lines);
                        let head_lines: Vec<String> = content.lines().take(outline_limit).enumerate().map(|(i, line)| {
                            let truncated = if line.len() > MAX_LINE_LENGTH {
                                format!("{}... [truncated]", &line[..MAX_LINE_LENGTH])
                            } else { line.to_string() };
                            format!("{:>6}|{}", i + 1, truncated)
                        }).collect();
                        let head_size = head_lines.iter().map(|l| l.len() + 1).sum::<usize>();
                        let truncated = if head_size > OUTPUT_MAX_BYTES {
                            let mut byte_sum = 0usize;
                            let mut keep_count = 0usize;
                            for line in &head_lines {
                                byte_sum += line.len() + 1;
                                if byte_sum > OUTPUT_MAX_BYTES { break; }
                                keep_count += 1;
                            }
                            head_lines[..keep_count].join("\n")
                        } else {
                            head_lines.join("\n")
                        };
                        return Ok(format!(
                            "<path>{}</path>\n<type>file</type>\n<size>{} bytes</size>\n<lines>{}</lines>\n\n\
                             [Large file: {} bytes, {} lines — outline mode (threshold {}KB)]\n\n\
                             [Head {} lines for orientation]\n{}\n\n\
                             [To read more, use:\n\
                              - read_file path=\"{}\" range=\"A-B\"    — 1-indexed line range\n\
                              - read_file path=\"{}\" head:N / tail:N  — first/last N lines]",
                            path_str, file_size, total_lines,
                            format_file_size(file_size), total_lines, FULL_READ_THRESHOLD_BYTES / 1024,
                            outline_limit, truncated,
                            path_str, path_str
                        ));
                    }

                    // ── 核心读取 ──
                    let lines: Vec<String> = content.lines()
                        .skip(start_line)
                        .take(read_limit)
                        .enumerate()
                        .map(|(i, line)| {
                            let line_num = start_line + i + 1;
                            // 每行截断
                            if line.len() > MAX_LINE_LENGTH {
                                format!("{:>6}|{}... [truncated]", line_num, &line[..MAX_LINE_LENGTH])
                            } else {
                                format!("{:>6}|{}", line_num, line)
                            }
                        })
                        .collect();

                    // ── 字节预算检查 ──
                    let mut byte_sum = 0usize;
                    let mut keep_count = 0usize;
                    let mut capped = false;
                    for line in &lines {
                        let line_bytes = line.len() + 1; // +1 for newline
                        if byte_sum + line_bytes > OUTPUT_MAX_BYTES {
                            capped = true;
                            break;
                        }
                        byte_sum += line_bytes;
                        keep_count += 1;
                    }
                    let last_line = start_line + keep_count;
                    let output_lines = if capped { &lines[..keep_count] } else { &lines[..] };

                    // 构建输出
                    let mut output = format!("<path>{}</path>\n<type>file</type>\n", path_str);
                    if output_lines.is_empty() {
                        output.push_str(&format!("<lines>0</lines>\n\n(Empty file - {} lines total)", total_lines));
                    } else {
                        output.push_str(&format!("<lines>{}</lines>\n\n", total_lines));
                        output.push_str(&output_lines.join("\n"));
                        // 截断提示
                        if capped {
                            output.push_str(&format!(
                                "\n\n(Output capped at {}KB. Showing lines {}-{} of {}. Use offset={} to continue.)",
                                OUTPUT_MAX_BYTES / 1024,
                                start_line + 1, last_line, total_lines, last_line + 1
                            ));
                        } else if last_line < total_lines {
                            output.push_str(&format!(
                                "\n\n(Showing lines {}-{} of {}. Use offset={} to continue.)",
                                start_line + 1, last_line, total_lines, last_line + 1
                            ));
                        } else {
                            output.push_str(&format!(
                                "\n\n(End of file - total {} lines)", total_lines
                            ));
                        }
                    }

                    Ok(output)
                },
            ),
        })
        .await;

    // write_file tool
    registry
        .register(ToolDef {
                        name: "write_file".to_string(),
            display_name: "写入文件".to_string(),
            description:
                "Write content to a file (auto-creates dirs). Params: path (required), content (required)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to working directory, absolute paths used directly)"
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

                    // 计算新旧文件行数变化
                    let new_lines = content.lines().count();
                    let old_lines = if resolved_path.exists() {
                        std::fs::read_to_string(&resolved_path)
                            .map(|s| s.lines().count())
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let added = if new_lines > old_lines { new_lines - old_lines } else { 0 };
                    let removed = if old_lines > new_lines { old_lines - new_lines } else { 0 };
                    // 写入新内容后，行数变化总是 ±new_lines（覆盖写入）
                    // 但用户直观想看写入后的行数，以及相对于旧文件的增减
                    // 格式: "+N -M"
                    let diff_label = if old_lines == 0 {
                        format!("+{} -0", new_lines)
                    } else if new_lines > old_lines {
                        format!("+{} -0", added)
                    } else if new_lines < old_lines {
                        format!("+0 -{}", removed)
                    } else {
                        format!("+{} -0", new_lines)
                    };

                    if let Some(parent) = resolved_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create directory: {}", e))?;
                    }
                    std::fs::write(&resolved_path, content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    // 写入后重置该文件的重复读取计数
                    reset_read_repeat_counter(&resolved_path.to_string_lossy());

                    Ok(format!("{} {}", diff_label, resolved_path.display()))
                },
            ),
        })
        .await;

    // edit_file tool
    registry
        .register(ToolDef {
                        name: "edit_file".to_string(),
            display_name: "编辑文件".to_string(),
            description:
                "Find and replace text in a file (1 replacement). Params: path (required), old_string (required), new_string (required)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative paths resolve to working directory, absolute paths used directly)"
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

                    // 计算行数变化
                    let old_lines = content.lines().count();
                    let new_content = content.replacen(old_str, new_str, 1);
                    let new_lines = new_content.lines().count();
                    let diff_label = if new_lines > old_lines {
                        format!("+{} -0", new_lines - old_lines)
                    } else if new_lines < old_lines {
                        format!("+0 -{}", old_lines - new_lines)
                    } else {
                        "+0 -0".to_string()
                    };

                    std::fs::write(&resolved_path, &new_content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    // 编辑后重置该文件的重复读取计数
                    reset_read_repeat_counter(&resolved_path.to_string_lossy());

                    Ok(format!("{} {}", diff_label, resolved_path.display()))
                },
            ),
        })
        .await;

    // glob file search tool
    registry
        .register(ToolDef {
                        name: "glob".to_string(),
            display_name: "搜索文件".to_string(),
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
                        "description": "Root directory to search (relative paths resolve to working directory, absolute paths used directly); defaults to current directory"
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
            display_name: "搜索文本".to_string(),
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
                        "description": "Directory to search (relative paths resolve to working directory, absolute paths used directly); defaults to current directory"
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
            display_name: "批量替换".to_string(),
            description:
                "Batch find and replace text across multiple files using regex. Params: pattern (required, regex), replacement (required), path (optional directory, defaults to working directory), include (optional file filter like '*.rs' or '*.tsx')"
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
                        "description": "Directory to search in (omit to use current working directory)"
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
            display_name: "列出目录".to_string(),
            description:
                "List files and directories. Params: path (optional, defaults to '.' — the working directory). Returns directory entries with name, type, and size"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path (relative to working directory or absolute; omit to list current directory)"
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
                    let base = match base {
                        "" | "." | "workspace" | "workspace/" | "workspace\\" => ".",
                        other => other,
                    };
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
            display_name: "重命名文件".to_string(),
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
