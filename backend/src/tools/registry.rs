use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 工具定义（含 OpenAI Function Calling Schema）
#[derive(Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub handler: Arc<dyn Fn(Value) -> Result<String, String> + Send + Sync>,
}

/// 工具注册表
#[derive(Clone)]
pub struct ToolRegistry {
    pub(crate) tools: Arc<RwLock<HashMap<String, ToolDef>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl ToolRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册工具
    pub async fn register(&self, tool: ToolDef) {
        let mut tools = self.tools.write().await;
        tracing::info!("注册工具: {}", tool.name);
        tools.insert(tool.name.clone(), tool);
    }

    /// 获取工具定义
    pub async fn get(&self, name: &str) -> Option<ToolDef> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// 执行工具
    pub async fn execute(&self, name: &str, args: Value) -> Result<String, String> {
        let tools = self.tools.read().await;
        match tools.get(name) {
            Some(tool) => (tool.handler)(args),
            None => Err(format!("Unknown tool: {}", name)),
        }
    }

    /// 获取所有工具的 LLM Schema
    pub async fn get_schemas(&self) -> Vec<super::types::ToolDefinition> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|t| super::types::ToolDefinition {
                def_type: "function".to_string(),
                function: super::types::FunctionDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    /// 获取工具数量
    pub async fn count(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// 检查工具是否存在
    pub async fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }
}
