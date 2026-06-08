use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

/// 应用配置（项目配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// HTTP 服务器端口
    #[serde(default)]
    pub port: u16,
    /// 监听地址
    #[serde(default)]
    pub host: String,
    /// LLM 请求超时（秒）
    #[serde(default)]
    pub llm_timeout: u32,
    /// 最大重试次数
    #[serde(default)]
    pub max_retries: u32,
    /// 最大 Agent 迭代次数（0=无限制）
    #[serde(default)]
    pub max_iterations: usize,
    /// Agent 温度参数
    #[serde(default)]
    pub temperature: f64,
    /// 上下文压缩阈值（消息数超过此值时触发压缩）
    #[serde(default)]
    pub compact_threshold: usize,
    /// 上下文压缩保留消息数
    #[serde(default)]
    pub compact_keep: usize,
    /// 允许的来源（CORS）
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Prompt 注入保护开关
    #[serde(default)]
    pub prompt_injection_protection: bool,
    /// 数据目录（可选，用于自定义路径）
    pub data_dir: Option<String>,
    /// TinyFish Search API KEY（可选，用于网络搜索）
    pub tinyfish_api_key: Option<String>,
    /// Tavily Search API KEY（可选，用于网络搜索）
    pub tavily_api_key: Option<String>,
    /// 命令黑名单正则表达式列表（匹配到的命令被阻止执行）
    #[serde(default)]
    pub deny_patterns: Vec<String>,
    /// 命令白名单前缀列表（匹配到的命令跳过审批直接执行）
    #[serde(default)]
    pub shell_allowlist: Vec<String>,
    /// 技能启用状态映射 { skill_name: enabled }
    #[serde(default)]
    pub skills: HashMap<String, bool>,
    /// 命令审批模式: "approval"（需审批）或 "auto"（自动执行）
    #[serde(default)]
    pub approval_mode: String,
}

/// 模型配置（单独存放）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// 默认模型名称
    pub default_model: String,
    /// LLM 提供商列表
    pub providers: Vec<ProviderConfig>,
}

/// 单个模型条目（支持旧格式字符串和新格式对象）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub context_window: Option<u64>,
}

impl ModelConfig {
    pub fn new(name: String) -> Self {
        Self { name, context_window: None }
    }
}

/// 模型条目枚举，用于兼容旧版字符串格式和新的对象格式
#[derive(Debug, Clone)]
pub enum ModelEntry {
    Name(String),
    Config(ModelConfig),
}

impl ModelEntry {
    pub fn name(&self) -> &str {
        match self {
            ModelEntry::Name(n) => n,
            ModelEntry::Config(c) => &c.name,
        }
    }

    pub fn context_window(&self) -> Option<u64> {
        match self {
            ModelEntry::Name(_) => None,
            ModelEntry::Config(c) => c.context_window,
        }
    }
}

impl<'de> Deserialize<'de> for ModelEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        use serde::de;
        struct ModelEntryVisitor;
        impl<'de> de::Visitor<'de> for ModelEntryVisitor {
            type Value = ModelEntry;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or a model config object")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<ModelEntry, E> {
                Ok(ModelEntry::Name(value.to_string()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<ModelEntry, E> {
                Ok(ModelEntry::Name(value))
            }
            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<ModelEntry, M::Error> {
                let config = ModelConfig::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ModelEntry::Config(config))
            }
        }
        deserializer.deserialize_any(ModelEntryVisitor)
    }
}

impl Serialize for ModelEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        let config = match self {
            ModelEntry::Name(name) => ModelConfig { name: name.clone(), context_window: None },
            ModelEntry::Config(cfg) => cfg.clone(),
        };
        config.serialize(serializer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供商名称
    pub name: String,
    /// API Key
    pub api_key: String,
    /// API Base URL
    pub base_url: String,
    /// 模型列表（支持字符串和对象格式）
    pub models: Vec<ModelEntry>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 5173,
            host: "127.0.0.1".to_string(),
            llm_timeout: 60,
            max_retries: 1,
            max_iterations: 0, // 0=无硬上限，由上下文使用率驱动循环
            temperature: 0.7,
            compact_threshold: 100,
            compact_keep: 40,
            allowed_origins: vec![

                "http://localhost:1420".to_string(),
                "http://localhost:5173".to_string(),
                "http://127.0.0.1:1420".to_string(),
                "http://127.0.0.1:5173".to_string(),
                "tauri://localhost".to_string(),
            ],
            prompt_injection_protection: true,
            data_dir: None,
            tinyfish_api_key: None,
            tavily_api_key: None,
            deny_patterns: vec![
                // ─── 仅保留真正高危的命令 ───
                "rm -rf /".into(), "rmdir /s /q".into(), "del /f /s /q".into(),
                "format ".into(), "dd if=/dev/zero".into(), "dd if=/dev/random".into(),
                "mkfs".into(), "diskpart".into(), "fdisk".into(),
                "shutdown".into(), "reboot".into(), "poweroff".into(), "halt".into(),
                "sudo rm".into(), "chmod 777".into(), "chmod -R 777".into(),
                "eval".into(), "exec".into(),
                // ─── 远程连接（需要用户确认而非直接禁止，但高危参数直接拒） ───
                "ssh -o StrictHostKeyChecking=no".into(),
            ],
            shell_allowlist: vec![],
            skills: HashMap::new(),
            approval_mode: "approval".to_string(),
        }
    }
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            default_model: String::new(),
            providers: vec![],
        }
    }
}

impl AppConfig {
    /// 从配置文件加载，不存在则创建默认配置
    pub fn load() -> Self {
        let config_path = Self::config_path();
        tracing::info!("项目配置文件路径: {:?}", config_path);

        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str::<Self>(&content) {
                        Ok(config) => {
                            // 如果旧配置缺少 deny_patterns 或 profiles，注入默认值
                            let mut config = config;
                            if config.deny_patterns.is_empty() {
                                config.deny_patterns = Self::default().deny_patterns;
                                tracing::info!("已注入默认命令黑名单到项目配置");
                            }
                            let _ = config.save();
                            tracing::info!("成功从 {:?} 加载项目配置", config_path);
                            return config;
                        }
                        Err(e) => {
                            tracing::error!(
                                "解析项目配置文件失败: {} (路径: {:?})，将使用默认配置",
                                e,
                                config_path
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("读取项目配置文件失败: {} (路径: {:?})", e, config_path);
                }
            }
        } else {
            tracing::info!("项目配置文件不存在 {:?}，将创建默认配置", config_path);
        }

        let config = Self::default();
        if let Err(e) = config.save() {
            tracing::warn!("保存项目配置失败: {}", e);
        } else {
            tracing::info!("已创建默认项目配置文件 {:?}", config_path);
        }
        config
    }

    /// 从文件重新加载配置
    pub fn reload() -> Self {
        let config_path = Self::config_path();
        tracing::info!("重新加载项目配置: {:?}", config_path);

        if !config_path.exists() {
            tracing::warn!("项目配置文件不存在，返回默认配置");
            return Self::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match serde_json::from_str::<Self>(&content) {
                    Ok(mut config) => {
                        // 如果旧配置缺少 deny_patterns，自动注入默认值
                        if config.deny_patterns.is_empty() {
                            config.deny_patterns = Self::default().deny_patterns;
                            let _ = config.save();
                            tracing::info!("已注入默认命令黑名单到项目配置");
                        }
                        tracing::info!("重新加载项目配置成功");
                        config
                    }
                    Err(e) => {
                        tracing::error!("重新加载项目配置解析失败: {}", e);
                        Self::default()
                    }
                }
            }
            Err(e) => {
                tracing::error!("重新加载项目配置读取失败: {}", e);
                Self::default()
            }
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<(), anyhow::Error> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content)?;
        tracing::info!("项目配置保存到 {:?}", config_path);
        Ok(())
    }

    /// 项目配置文件路径
    pub fn config_path() -> PathBuf {
        if let Ok(path) = std::env::var("JEEVES_CONFIG") {
            return PathBuf::from(path);
        }
        get_config_dir().join("config.json")
    }

    /// 数据目录路径（基础数据目录）
    pub fn data_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.data_dir {
            return PathBuf::from(dir);
        }
        get_base_dir()
    }

    /// 工作目录（内容产出、文件读写）
    pub fn workspace_dir(&self) -> PathBuf {
        get_workspace_dir()
    }

    /// 技能存放路径
    pub fn skills_dir(&self) -> PathBuf {
        get_skills_dir()
    }

    /// 记忆存放路径
    pub fn memories_dir(&self) -> PathBuf {
        get_memories_dir()
    }

    /// 会话存放路径
    pub fn sessions_dir(&self) -> PathBuf {
        get_sessions_dir()
    }
}

impl ModelsConfig {
    /// 从配置文件加载模型配置
    pub fn load() -> Self {
        let models_path = Self::models_path();
        tracing::info!("模型配置文件路径: {:?}", models_path);

        if models_path.exists() {
            match fs::read_to_string(&models_path) {
                Ok(content) => {
                    match serde_json::from_str::<Self>(&content) {
                        Ok(config) => {
                            tracing::info!(
                                "成功从 {:?} 加载模型配置 ({} 个提供商)",
                                models_path,
                                config.providers.len()
                            );
                            return config;
                        }
                        Err(e) => {
                            tracing::error!(
                                "解析模型配置文件失败: {} (路径: {:?})，将使用默认配置",
                                e,
                                models_path
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("读取模型配置文件失败: {} (路径: {:?})", e, models_path);
                }
            }
        } else {
            tracing::info!("模型配置文件不存在 {:?}，将创建默认配置", models_path);
        }

        let config = Self::default();
        if let Err(e) = config.save() {
            tracing::warn!("保存模型配置失败: {}", e);
        } else {
            tracing::info!("已创建默认模型配置文件 {:?}", models_path);
        }
        config
    }

    /// 从文件重新加载模型配置
    pub fn reload() -> Self {
        let models_path = Self::models_path();
        tracing::info!("重新加载模型配置: {:?}", models_path);

        if !models_path.exists() {
            tracing::warn!("模型配置文件不存在，创建默认配置");
            let config = Self::default();
            if let Err(e) = config.save() {
                tracing::warn!("创建默认模型配置失败: {}", e);
            } else {
                tracing::info!("已创建默认模型配置文件");
            }
            return config;
        }

        match fs::read_to_string(&models_path) {
            Ok(content) => {
                match serde_json::from_str::<Self>(&content) {
                    Ok(config) => {
                        tracing::info!(
                            "重新加载模型配置成功 ({} 个提供商)",
                            config.providers.len()
                        );
                        config
                    }
                    Err(e) => {
                        tracing::error!("重新加载模型配置解析失败: {}", e);
                        Self::default()
                    }
                }
            }
            Err(e) => {
                tracing::error!("重新加载模型配置读取失败: {}", e);
                Self::default()
            }
        }
    }

    /// 保存模型配置到文件
    pub fn save(&self) -> Result<(), anyhow::Error> {
        let models_path = Self::models_path();
        if let Some(parent) = models_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&models_path, content)?;
        tracing::info!("模型配置保存到 {:?}", models_path);
        Ok(())
    }

    /// 模型配置文件路径
    pub fn models_path() -> PathBuf {
        if let Ok(path) = std::env::var("JEEVES_MODELS_CONFIG") {
            return PathBuf::from(path);
        }
        get_config_dir().join("models.json")
    }

    /// 获取指定名称的提供商配置
    pub fn find_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.name == name)
    }

    /// 根据模型名称匹配提供商
    pub fn find_provider_by_model(&self, model: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.models.iter().any(|m| m.name() == model))
    }

    /// 获取默认提供商（需要传入默认模型名称）
    pub fn default_provider(&self, default_model: &str) -> Option<&ProviderConfig> {
        self.find_provider_by_model(default_model)
            .or_else(|| self.providers.first())
    }
}

    /// 获取应用根目录
    ///
    /// | 平台   | 路径示例                                  |
    /// |--------|-------------------------------------------|
    /// | Win    | %USERPROFILE%\Documents\jeeves\          |
    /// | macOS  | ~/Documents/jeeves/                      |
    /// | Linux  | ~/.local/share/jeeves/                   |
    pub fn get_base_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            // 使用用户文档目录，更友好且非隐藏
            std::env::var("USERPROFILE")
                .map(|p| PathBuf::from(p).join("Documents").join("jeeves"))
                .unwrap_or_else(|_| {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Documents")
                        .join("jeeves")
                })
        }

        #[cfg(target_os = "macos")]
        {
            dirs::document_dir()
                .unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Documents")
                })
                .join("jeeves")
        }

    #[cfg(target_os = "linux")]
    {
        let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local")
                .join("share")
                .display()
                .to_string()
        });
        PathBuf::from(data_home).join("jeeves")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("jeeves")
    }
}

/// 获取配置目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\config\    |
/// | macOS  | ~/Documents/jeeves/config/                |
/// | Linux  | ~/.config/jeeves/                        |
pub fn get_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        get_base_dir().join("config")
    }

    #[cfg(target_os = "macos")]
    {
        get_base_dir().join("config")
    }

    #[cfg(target_os = "linux")]
    {
        let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
                .display()
                .to_string()
        });
        PathBuf::from(config_home).join("jeeves")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        get_base_dir().join("config")
    }
}

/// 获取工作目录（内容产出、文件读写）
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\workspace\ |
/// | macOS  | ~/Documents/jeeves/workspace/              |
/// | Linux  | ~/.local/share/jeeves/workspace/         |
pub fn get_workspace_dir() -> PathBuf {
    get_base_dir().join("workspace")
}

/// 获取技能存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\skills\    |
/// | macOS  | ~/Documents/jeeves/skills/                 |
/// | Linux  | ~/.local/share/jeeves/skills/            |
pub fn get_skills_dir() -> PathBuf {
    get_base_dir().join("skills")
}

/// 获取记忆存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\memories\ |
/// | macOS  | ~/Library/Application Support/jeeves/memories/ |
/// | Linux  | ~/.local/share/jeeves/memories/          |
pub fn get_memories_dir() -> PathBuf {
    get_base_dir().join("memories")
}

/// 获取会话存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\sessions\ |
/// | macOS  | ~/Library/Application Support/jeeves/sessions/ |
/// | Linux  | ~/.local/share/jeeves/sessions/          |
pub fn get_sessions_dir() -> PathBuf {
    get_base_dir().join("sessions")
}

/// 获取媒体文件入站目录（按会话ID区分）
///
/// 用于存放用户发送的附件（文件、视频等），按会话 ID 分目录管理，
/// 删除会话时同步删除对应目录。
///
/// | 平台   | 路径示例                                                        |
/// |--------|-----------------------------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\sessions\media\inbound\{session_id}\ |
/// | macOS  | ~/Library/Application Support/jeeves/sessions/media/inbound/{session_id}/ |
/// | Linux  | ~/.local/share/jeeves/sessions/media/inbound/{session_id}/       |
pub fn get_media_inbound_dir(session_id: &str) -> PathBuf {
    get_sessions_dir().join("media").join("inbound").join(session_id)
}

/// 获取媒体文件临时目录
///
/// 用于存放临时媒体文件（如图片解密后的临时缓存），
/// 应用重启时可能清理。
///
/// | 平台   | 路径示例                                              |
/// |--------|-------------------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\sessions\media\temp\ |
/// | macOS  | ~/Library/Application Support/jeeves/sessions/media/temp/ |
/// | Linux  | ~/.local/share/jeeves/sessions/media/temp/           |
pub fn get_media_temp_dir() -> PathBuf {
    get_sessions_dir().join("media").join("temp")
}

/// 获取日志存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\logs\     |
/// | macOS  | ~/Library/Application Support/jeeves/logs/ |
/// | Linux  | ~/.local/share/jeeves/logs/              |
pub fn get_logs_dir() -> PathBuf {
    get_base_dir().join("logs")
}

/// 获取定时任务存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %USERPROFILE%\Documents\jeeves\cron\     |
/// | macOS  | ~/Library/Application Support/jeeves/cron/ |
/// | Linux  | ~/.local/share/jeeves/cron/              |
pub fn get_cron_dir() -> PathBuf {
    get_base_dir().join("cron")
}

/// 确保所有必要目录存在，不存在则创建
pub fn ensure_directories_exists() {
    let dirs = [
        get_base_dir(),
        get_config_dir(),
        get_workspace_dir(),
        get_skills_dir(),
        get_memories_dir(),
        get_sessions_dir(),
        get_logs_dir(),
        get_cron_dir(),
    ];

    tracing::info!("=== 目录初始化 ===");
    tracing::info!("基础目录: {:?}", get_base_dir());
    tracing::info!("配置目录: {:?}", get_config_dir());
    tracing::info!("工作目录: {:?}", get_workspace_dir());
    tracing::info!("技能目录: {:?}", get_skills_dir());
    tracing::info!("记忆目录: {:?}", get_memories_dir());
    tracing::info!("会话目录: {:?}", get_sessions_dir());
    tracing::info!("日志目录: {:?}", get_logs_dir());
    tracing::info!("定时任务目录: {:?}", get_cron_dir());
    tracing::info!("=================");

    for dir in dirs {
        if !dir.exists() {
            tracing::info!("目录不存在，尝试创建: {:?}", dir);
            match fs::create_dir_all(&dir) {
                Ok(_) => tracing::info!("成功创建目录: {:?}", dir),
                Err(e) => tracing::error!("创建目录失败: {} - {:?}", e, dir),
            }
        } else {
            tracing::info!("目录已存在: {:?}", dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_app_config() {
        let config = AppConfig::default();
        assert_eq!(config.port, 5173);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_default_models_config() {
        let config = ModelsConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.default_model.is_empty());
    }

    #[test]
    fn test_find_provider_by_model() {
        let config = ModelsConfig {
            default_model: "gpt-4".to_string(),
            providers: vec![ProviderConfig {
                name: "openai".to_string(),
                api_key: "test-key".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                models: vec![ModelEntry::Name("gpt-4".to_string()), ModelEntry::Name("gpt-3.5-turbo".to_string())],
            }],
        };
        let provider = config.find_provider_by_model("gpt-4");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name, "openai");
    }
}
