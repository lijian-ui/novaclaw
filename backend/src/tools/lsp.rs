use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

/// LSP 客户端 —— 管理语言服务器进程的生命周期与 JSON-RPC 通信
pub struct LspClient {
    process: Option<(ChildStdin, BufReader<ChildStdout>)>,
    server_cmd: String,
    initialized: bool,
    request_id: u64,
    server_capabilities: Option<HashMap<String, serde_json::Value>>,
}

impl LspClient {
    pub fn new(server_cmd: &str) -> Self {
        Self {
            process: None,
            server_cmd: server_cmd.to_string(),
            initialized: false,
            request_id: 0,
            server_capabilities: None,
        }
    }

    /// 启动语言服务器进程并完成初始化握手
    pub fn start(&mut self, root_uri: &str) -> Result<(), String> {
        let child = Command::new(&self.server_cmd)
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start language server '{}': {}", self.server_cmd, e))?;

        let stdin = child.stdin.unwrap();
        let stdout = BufReader::new(child.stdout.unwrap());
        self.process = Some((stdin, stdout));

        // 1. 初始化请求
        let init_params = serde_json::json!({
            "processId": null,
            "rootUri": root_uri,
            "capabilities": {}
        });
        let init_result = self.send_request("initialize", init_params)?;
        self.server_capabilities = init_result.get("capabilities")
            .and_then(|c| c.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());

        // 2. 发送 initialized 通知
        self.send_notification("initialized", serde_json::json!({}));

        self.initialized = true;
        Ok(())
    }

    /// 发送通知（无响应）
    fn send_notification(&mut self, method: &str, params: serde_json::Value) {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&msg);
    }

    /// 发送请求并等待响应
    fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        self.request_id += 1;
        let id = self.request_id;
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&msg);
        self.read_response(id)
    }

    /// 写入 JSON-RPC 消息（Content-Length 头 + JSON 体）
    fn write_message(&mut self, msg: &serde_json::Value) {
        if let Some((ref mut stdin, _)) = self.process {
            let body = msg.to_string();
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            let _ = stdin.write_all(header.as_bytes());
            let _ = stdin.write_all(body.as_bytes());
            let _ = stdin.flush();
        }
    }

    /// 读取指定 id 的 JSON-RPC 响应
    fn read_response(&mut self, expected_id: u64) -> Result<serde_json::Value, String> {
        if let Some((_, ref mut reader)) = self.process {
            loop {
                let mut content_length = 0usize;
                // 读取头部
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).map_err(|e| format!("LSP read error: {}", e))?;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break; // 空行，头部结束
                    }
                    if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                        content_length = len_str.parse::<usize>().unwrap_or(0);
                    }
                }

                // 读取 JSON 体
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body).map_err(|e| format!("LSP read body error: {}", e))?;
                let json: serde_json::Value = serde_json::from_slice(&body)
                    .map_err(|e| format!("LSP JSON parse error: {}", e))?;

                // 检查是否是我们等待的响应
                if let Some(resp_id) = json.get("id").and_then(|v| v.as_u64()) {
                    if resp_id == expected_id {
                        if let Some(err) = json.get("error") {
                            return Err(format!("LSP error: {}", err));
                        }
                        return Ok(json.get("result").cloned().unwrap_or(serde_json::Value::Null));
                    }
                }
                // 不是期待的响应（可能是通知或之前请求的响应），继续读
            }
        }
        Err("LSP process not started".to_string())
    }

    /// 获取定义位置
    pub fn goto_definition(&mut self, file_path: &str, line: u32, character: u32) -> Result<Vec<serde_json::Value>, String> {
        let params = serde_json::json!({
            "textDocument": { "uri": format!("file:///{}", file_path.replace('\\', "/")) },
            "position": { "line": line, "character": character }
        });
        let result = self.send_request("textDocument/definition", params)?;
        Ok(match result {
            serde_json::Value::Array(arr) => arr,
            serde_json::Value::Object(_) => vec![result],
            _ => vec![],
        })
    }

    /// 获取引用
    pub fn find_references(&mut self, file_path: &str, line: u32, character: u32) -> Result<Vec<serde_json::Value>, String> {
        let params = serde_json::json!({
            "textDocument": { "uri": format!("file:///{}", file_path.replace('\\', "/")) },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": true }
        });
        let result = self.send_request("textDocument/references", params)?;
        Ok(match result {
            serde_json::Value::Array(arr) => arr,
            _ => vec![],
        })
    }

    /// 获取诊断信息（重新打开文档触发诊断）
    pub fn get_diagnostics(&mut self, file_path: &str, content: &str) -> Result<Vec<serde_json::Value>, String> {
        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        // 打开文档
        let did_open = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": self.detect_language(file_path),
                "version": 1,
                "text": content
            }
        });
        self.send_notification("textDocument/didOpen", did_open);

        // 请求诊断
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        let result = self.send_request("textDocument/diagnostic", params)?;

        Ok(result.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default())
    }

    /// 获取悬停信息
    pub fn hover(&mut self, file_path: &str, line: u32, character: u32) -> Result<String, String> {
        let params = serde_json::json!({
            "textDocument": { "uri": format!("file:///{}", file_path.replace('\\', "/")) },
            "position": { "line": line, "character": character }
        });
        let result = self.send_request("textDocument/hover", params)?;
        let contents = match result.get("contents") {
            Some(c) => {
                if let Some(v) = c.get("value").and_then(|v| v.as_str()) {
                    v.to_string()
                } else if let Some(s) = c.as_str() {
                    s.to_string()
                } else if let Some(arr) = c.as_array() {
                    arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n")
                } else {
                    String::new()
                }
            }
            None => String::new(),
        };
        Ok(contents)
    }

    /// 关闭连接
    pub fn shutdown(&mut self) {
        if self.initialized {
            let _ = self.send_request("shutdown", serde_json::json!(null));
            self.send_notification("exit", serde_json::json!({}));
        }
        self.process = None;
    }

    fn detect_language(&self, path: &str) -> &'static str {
        if path.ends_with(".rs") { "rust" }
        else if path.ends_with(".ts") || path.ends_with(".tsx") { "typescript" }
        else if path.ends_with(".js") || path.ends_with(".jsx") { "javascript" }
        else if path.ends_with(".py") { "python" }
        else if path.ends_with(".go") { "go" }
        else if path.ends_with(".java") { "java" }
        else if path.ends_with(".css") { "css" }
        else if path.ends_with(".json") { "json" }
        else if path.ends_with(".md") { "markdown" }
        else { "plaintext" }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 从文件路径和内容中寻找最佳的行号/字符位置
pub fn find_position(content: &str, symbol: &str, line_hint: Option<u32>) -> (u32, u32) {
    if let Some(lh) = line_hint {
        let line = lh.min(content.lines().count().saturating_sub(1) as u32);
        let col = content.lines().nth(line as usize)
            .map(|l| l.find(symbol).unwrap_or(0) as u32)
            .unwrap_or(0);
        return (line, col);
    }
    for (i, line) in content.lines().enumerate() {
        if let Some(col) = line.find(symbol) {
            return (i as u32, col as u32);
        }
    }
    (0, 0)
}
