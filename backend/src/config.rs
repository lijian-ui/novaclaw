use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

/// 应用配置（项目配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// HTTP 服务器端口
    pub port: u16,
    /// 监听地址
    pub host: String,
    /// LLM 请求超时（秒）
    pub llm_timeout: u32,
    /// 最大重试次数
    pub max_retries: u32,
    /// 最大 Agent 迭代次数
    pub max_iterations: usize,
    /// Agent 温度参数
    pub temperature: f64,
    /// 允许的来源（CORS）
    pub allowed_origins: Vec<String>,
    /// Prompt 注入保护开关
    pub prompt_injection_protection: bool,
    /// 数据目录（可选，用于自定义路径）
    pub data_dir: Option<String>,
    /// TinyFish Search API KEY（可选，用于网络搜索）
    pub tinyfish_api_key: Option<String>,
    /// Tavily Search API KEY（可选，用于网络搜索）
    pub tavily_api_key: Option<String>,
}

/// 模型配置（单独存放）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// 默认模型名称
    pub default_model: String,
    /// LLM 提供商列表
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供商名称
    pub name: String,
    /// API Key
    pub api_key: String,
    /// API Base URL
    pub base_url: String,
    /// 模型列表
    pub models: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "127.0.0.1".to_string(),
            llm_timeout: 60,
            max_retries: 3,
            max_iterations: 30,
            temperature: 0.7,
            allowed_origins: vec![
                "http://localhost:1420".to_string(),
                "http://localhost:5173".to_string(),
                "tauri://localhost".to_string(),
            ],
            prompt_injection_protection: true,
            data_dir: None,
            tinyfish_api_key: None,
            tavily_api_key: None,
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
                    Ok(config) => {
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
        if let Ok(path) = std::env::var("NOVACLAW_CONFIG") {
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
        if let Ok(path) = std::env::var("NOVACLAW_MODELS_CONFIG") {
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
        self.providers.iter().find(|p| p.models.contains(&model.to_string()))
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
/// | Win    | %LOCALAPPDATA%\novaclaw\                   |
/// | macOS  | ~/Library/Application Support/novaclaw/    |
/// | Linux  | ~/.local/share/novaclaw/                   |
pub fn get_base_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("AppData")
                .join("Local")
                .display()
                .to_string()
        });
        PathBuf::from(local_app_data).join("novaclaw")
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join("Library")
            .join("Application Support")
            .join("novaclaw")
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
        PathBuf::from(data_home).join("novaclaw")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("novaclaw")
    }
}

/// 获取配置目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %LOCALAPPDATA%\novaclaw\config\            |
/// | macOS  | ~/Library/Application Support/novaclaw/config/ |
/// | Linux  | ~/.config/novaclaw/                        |
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
        PathBuf::from(config_home).join("novaclaw")
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
/// | Win    | %LOCALAPPDATA%\novaclaw\workspace\         |
/// | macOS  | ~/Library/Application Support/novaclaw/workspace/ |
/// | Linux  | ~/.local/share/novaclaw/workspace/         |
pub fn get_workspace_dir() -> PathBuf {
    get_base_dir().join("workspace")
}

/// 获取技能存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %LOCALAPPDATA%\novaclaw\skills\            |
/// | macOS  | ~/Library/Application Support/novaclaw/skills/ |
/// | Linux  | ~/.local/share/novaclaw/skills/            |
pub fn get_skills_dir() -> PathBuf {
    get_base_dir().join("skills")
}

/// 获取记忆存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %LOCALAPPDATA%\novaclaw\memories\          |
/// | macOS  | ~/Library/Application Support/novaclaw/memories/ |
/// | Linux  | ~/.local/share/novaclaw/memories/          |
pub fn get_memories_dir() -> PathBuf {
    get_base_dir().join("memories")
}

/// 获取会话存放目录
/// 
/// | 平台   | 路径示例                                  |
/// |--------|-------------------------------------------|
/// | Win    | %LOCALAPPDATA%\novaclaw\sessions\          |
/// | macOS  | ~/Library/Application Support/novaclaw/sessions/ |
/// | Linux  | ~/.local/share/novaclaw/sessions/          |
pub fn get_sessions_dir() -> PathBuf {
    get_base_dir().join("sessions")
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
    ];

    for dir in dirs {
        if !dir.exists() {
            match fs::create_dir_all(&dir) {
                Ok(_) => tracing::info!("创建目录: {:?}", dir),
                Err(e) => tracing::error!("创建目录失败: {} - {:?}", e, dir),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_app_config() {
        let config = AppConfig::default();
        assert_eq!(config.port, 3000);
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
                models: vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()],
            }],
        };
        let provider = config.find_provider_by_model("gpt-4");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name, "openai");
    }
}
