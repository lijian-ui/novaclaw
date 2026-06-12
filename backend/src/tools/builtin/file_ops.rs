use crate::tools::builtin::{glob_match, resolve_path};
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

/// 提取代码文件中的符号（函数、类等）
fn extract_symbols(content: &str, path: Option<&str>) -> String {
    // 优先尝试使用 Tree-sitter (目前支持 Rust)
    if let Some(p) = path {
        if p.ends_with(".rs") {
            if let Some(ts_outline) = extract_symbols_rust_treesitter(content) {
                return ts_outline;
            }
        }
    }

    use regex::Regex;
    // 匹配 Rust, JS/TS, Python, Go 等常见语言的定义
    let patterns = [
        // Rust: pub fn name, fn name, struct Name, enum Name, impl Name, trait Name
        r"(?m)^(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        r"(?m)^(?:pub\s+)?(?:struct|enum|trait|type|impl)\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        // JS/TS: export function name, function name, class Name, interface Name, const name = () =>
        r"(?m)^(?:export\s+)?(?:async\s+)?function\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        r"(?m)^(?:export\s+)?(?:class|interface|type|enum)\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        r"(?m)^(?:export\s+)?const\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*(?:async\s*)?\(",
        // Python: def name, class Name
        r"(?m)^def\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        r"(?m)^class\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        // Go: func name, type Name
        r"(?m)^func\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        r"(?m)^type\s+([a-zA-Z_][a-zA-Z0-9_]*)",
    ];

    let mut symbols = Vec::new();
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(content) {
                if let Some(m) = cap.get(0) {
                    let line_num = content[..m.start()].lines().count() + 1;
                    symbols.push(format!("{:>6}| {}", line_num, m.as_str().trim()));
                }
            }
        }
    }
    
    // 按行号排序
    symbols.sort_by_key(|s| {
        s.split('|').next().unwrap_or("0").trim().parse::<usize>().unwrap_or(0)
    });
    
    symbols.join("\n")
}

/// 使用 Tree-sitter 提取 Rust 符号大纲
fn extract_symbols_rust_treesitter(content: &str) -> Option<String> {
    use tree_sitter::{Parser, Query, QueryCursor};
    
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_rust::language()).ok()?;
    
    let tree = parser.parse(content, None)?;
    let root_node = tree.root_node();
    
    // 定义查询：提取函数、结构体、枚举、实现、Trait 等
    let query_str = r#"
        (function_item name: (identifier) @name) @item
        (struct_item name: (type_identifier) @name) @item
        (enum_item name: (type_identifier) @name) @item
        (trait_item name: (type_identifier) @name) @item
        (impl_item type: (_) @name) @item
        (type_item name: (type_identifier) @name) @item
        (mod_item name: (identifier) @name) @item
        (macro_definition name: (identifier) @name) @item
    "#;
    
    let query = Query::new(tree_sitter_rust::language(), query_str).ok()?;
    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, root_node, content.as_bytes());
    
    let mut symbols = Vec::new();
    for m in matches {
        if m.captures.len() >= 2 {
            let item_node = m.captures[0].node;
            let name_node = m.captures[1].node;
            
            let start_byte = name_node.start_byte();
            let end_byte = name_node.end_byte();
            
            // 安全地截取字符串，避免字符边界问题
            let mut end = end_byte;
            while !content.is_char_boundary(end) { end -= 1; }
            let name = &content[start_byte..end];
            
            let line_num = item_node.start_position().row + 1;
            let kind = item_node.kind().replace("_item", "");
            
            symbols.push(format!("{:>6}| {} {}", line_num, kind, name));
        }
    }
    
    if symbols.is_empty() {
        None
    } else {
        Some(symbols.join("\n"))
    }
}

/// 使用 Tree-sitter 在 Rust 文件中查找指定符号的行号范围
fn find_symbol_range_rust(content: &str, target_name: &str) -> Option<(usize, usize)> {
    use tree_sitter::{Parser, Query, QueryCursor};
    
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_rust::language()).ok()?;
    
    let tree = parser.parse(content, None)?;
    let root_node = tree.root_node();
    
    // 查询所有具名项及其名称
    let query_str = r#"
        (function_item name: (identifier) @name) @item
        (struct_item name: (type_identifier) @name) @item
        (enum_item name: (type_identifier) @name) @item
        (trait_item name: (type_identifier) @name) @item
        (impl_item type: (_) @name) @item
        (type_item name: (type_identifier) @name) @item
        (mod_item name: (identifier) @name) @item
        (macro_definition name: (identifier) @name) @item
    "#;
    
    let query = Query::new(tree_sitter_rust::language(), query_str).ok()?;
    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, root_node, content.as_bytes());
    
    for m in matches {
        if m.captures.len() >= 2 {
            let item_node = m.captures[0].node;
            let name_node = m.captures[1].node;
            
            let start_byte = name_node.start_byte();
            let end_byte = name_node.end_byte();
            
            // 安全截取
            let mut end = end_byte;
            while !content.is_char_boundary(end) { end -= 1; }
            let name = &content[start_byte..end];
            
            if name == target_name {
                let start_line = item_node.start_position().row + 1;
                let end_line = item_node.end_position().row + 1;
                return Some((start_line, end_line));
            }
        }
    }
    
    None
}

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
            description: "Read file content. Supports range, head/tail, outline and symbol-based reading. \
                          Default returns FULL CONTENT for files ≤ 64KB. \
                          Larger files auto-switch to outline mode (head 80 lines + symbol outline). \
                          Use 'range' to read a specific line range (e.g. \"50-100\"), \
                          'head'/'tail' for first/last N lines.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" },
                    "offset": { "type": "integer", "description": "Start line (1-based)", "default": 1 },
                    "limit": { "type": "integer", "description": "Number of lines to read", "default": 2000 },
                    "range": { "type": "string", "description": "Inclusive line range like \"50-100\" or \"50-50\". 1-indexed. Takes precedence over offset/limit." },
                    "head": { "type": "integer", "description": "If set, return only the first N lines." },
                    "tail": { "type": "integer", "description": "If set, return only the last N lines." },
                    "outline": { "type": "boolean", "description": "If true, only returns symbols/outline of the file", "default": false },
                    "symbol": { "type": "string", "description": "Read a specific symbol definition (function, struct, etc.) precisely using Tree-sitter" }
                },
                "required": ["path"]
            }),

            skip_truncation_save: true,
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let resolved_path = resolve_path(path, &args);
                    let path_str = resolved_path.to_string_lossy().to_string();

                    // 打印详细参数日志，方便观察 LLM 是否使用了 symbol 等高级功能
                    tracing::info!(
                        "[Tool: read_file] path=\"{}\", symbol={:?}, outline={:?}, offset={:?}, limit={:?}, range={:?}, head={:?}, tail={:?}",
                        path_str,
                        args.get("symbol").and_then(|v| v.as_str()),
                        args.get("outline").and_then(|v| v.as_bool()),
                        args.get("offset").and_then(|v| v.as_u64()),
                        args.get("limit").and_then(|v| v.as_u64()),
                        args.get("range").and_then(|v| v.as_str()),
                        args.get("head").and_then(|v| v.as_u64()),
                        args.get("tail").and_then(|v| v.as_u64()),
                    );


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

                    // ── Outline 模式 ──
                    if args.get("outline").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let symbols = extract_symbols(&content, Some(path));
                        return Ok(format!(
                            "<path>{}</path>\n<type>file</type>\n<lines>{}</lines>\n<mode>outline</mode>\n\n{}",
                            path_str, total_lines, symbols
                        ));
                    }

                    // ── Symbol 模式 ──
                    if let Some(target_symbol) = args.get("symbol").and_then(|v| v.as_str()) {
                        if path.ends_with(".rs") {
                            if let Some((start_line, end_line)) = find_symbol_range_rust(&content, target_symbol) {
                                let lines: Vec<&str> = content.lines()
                                    .skip(start_line.saturating_sub(1))
                                    .take(end_line - start_line + 1)
                                    .collect();
                                let result = lines.join("\n");
                                return Ok(format!(
                                    "<path>{}</path>\n<type>file</type>\n<symbol>{}</symbol>\n<range>{}-{}</range>\n\n{}",
                                    path_str, target_symbol, start_line, end_line, result
                                ));
                            } else {
                                return Err(format!("Symbol '{}' not found in {}", target_symbol, path_str));
                            }
                        }
                    }

                    // ── 计算实际读取的行范围（参考 Reasonix 优先级: range > head/tail > offset/limit） ──
                    let (start_line, read_limit, source_desc) = {
                        // 1. range 参数 (如 "50-100" 或 "50-50") — 最高优先级
                        if let Some(range_str) = args.get("range").and_then(|v| v.as_str()) {
                            if let Some((raw_start, raw_end)) = range_str.split_once('-') {
                                let s = raw_start.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
                                let e = raw_end.trim().parse::<usize>().unwrap_or(total_lines).min(total_lines);
                                let count = e.saturating_sub(s).min(MAX_LINES);
                                (s, count, format!("range {}-{} of {} lines", s + 1, e, total_lines))
                            } else {
                                (0usize, MAX_LINES, format!("full file ({} lines)", total_lines))
                            }
                        // 2. head 参数 — 返回前 N 行
                        } else if let Some(n) = args.get("head").and_then(|v| v.as_u64()) {
                            let count = (n as usize).min(total_lines).min(MAX_LINES);
                            (0usize, count, format!("head {} of {} lines", count, total_lines))
                        // 3. tail 参数 — 返回后 N 行
                        } else if let Some(n) = args.get("tail").and_then(|v| v.as_u64()) {
                            let count = (n as usize).min(total_lines).min(MAX_LINES);
                            let start = total_lines.saturating_sub(count);
                            (start, count, format!("tail {} of {} lines", count, total_lines))
                        // 4. offset/limit 参数 — 默认值
                        } else {
                            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
                            let s = offset.saturating_sub(1);
                            let c = limit.min(MAX_LINES).min(total_lines.saturating_sub(s));
                            (s, c, format!("lines {}-{} of {}", s + 1, s + c, total_lines))
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

                    // ── 大文件检测：如果没有分片参数且文件 > 64KB，自动切换 outline 模式 ──
                    let has_scoping = args.get("range").is_some()
                        || args.get("head").is_some()
                        || args.get("tail").is_some()
                        || args.get("offset").map(|v| v.as_u64().unwrap_or(1) > 1).unwrap_or(false)
                        || args.get("limit").map(|v| v.as_u64().unwrap_or(2000) < MAX_LINES as u64).unwrap_or(false);
                    if !has_scoping && file_size > FULL_READ_THRESHOLD_BYTES && start_line == 0 && read_limit >= MAX_LINES {
                        // 返回大纲模式：前 80 行 + 文件信息 + 符号列表
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

                        let symbols = extract_symbols(&content, Some(&path));
                        let symbols_preview = if symbols.len() > 2000 {
                            format!("{}... (truncated)", crate::utils::safe_truncate(&symbols, 2000))
                        } else {
                            symbols
                        };

                        return Ok(format!(
                            "<path>{}</path>\n<type>file</type>\n<size>{} bytes</size>\n<lines>{}</lines>\n\n\
                             [Large file: {} bytes, {} lines — auto-outline mode (threshold {}KB)]\n\n\
                             [Head {} lines for orientation]\n{}\n\n\
                             [Major Symbols]\n{}\n\n\
                             [To read specific parts, use:\n\
                              - read_file path=\"{}\" range=\"A-B\"    — 1-indexed line range\n\
                              - read_file path=\"{}\" head:N / tail:N  — first/last N lines]",
                            path_str, file_size, total_lines,
                            format_file_size(file_size), total_lines, FULL_READ_THRESHOLD_BYTES / 1024,
                            outline_limit, truncated, symbols_preview,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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
            skip_truncation_save: false,
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

    // pin_file tool
    registry
        .register(ToolDef {
            name: "pin_file".to_string(),
            display_name: "固定文件".to_string(),
            description: "Pin a file's content to the persistent frozen context (System Prompt). \
                          Use this for core code files you need to refer to frequently. \
                          This reduces re-reading and ensures the content is always available in cache. \
                          Params: path (required)"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to pin"
                    }
                },
                "required": ["path"]
            }),
            skip_truncation_save: false,
            handler: std::sync::Arc::new(
                |args: serde_json::Value, _| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let resolved_path = resolve_path(path, &args);
                    if !resolved_path.exists() {
                        return Err(format!("File not found: {}", resolved_path.display()));
                    }
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;
                    
                    // 注意：这里的返回结果会被 AgentRuntime 拦截并执行真正的 pin 操作
                    Ok(format!("PIN_REQUEST:{}:{}", path, content))
                },
            ),
        })
        .await;

    // unpin_file tool
    registry
        .register(ToolDef {
            name: "unpin_file".to_string(),
            display_name: "取消固定".to_string(),
            description: "Remove a file from the persistent frozen context. Params: path (required)"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to unpin"
                    }
                },
                "required": ["path"]
            }),
            skip_truncation_save: false,
            handler: std::sync::Arc::new(
                |args: serde_json::Value, _| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    Ok(format!("UNPIN_REQUEST:{}", path))
                },
            ),
        })
        .await;
}
