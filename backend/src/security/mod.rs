//! 安全模块 - Prompt Injection 扫描与防护
//! 
//! 提供多层级的安全防护机制，检测和阻止恶意的 prompt 注入攻击

mod injection_scanner;
mod threat_patterns;

pub use injection_scanner::PromptInjectionScanner;
pub use threat_patterns::{ThreatPattern, ThreatSeverity};

/// 安全扫描结果
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// 是否安全通过
    pub safe: bool,
    /// 发现的威胁列表
    pub threats: Vec<ThreatReport>,
    /// 扫描耗时（毫秒）
    pub scan_duration_ms: u64,
}

/// 威胁报告
#[derive(Debug, Clone)]
pub struct ThreatReport {
    /// 威胁类型
    pub threat_type: String,
    /// 威胁描述
    pub description: String,
    /// 严重程度
    pub severity: ThreatSeverity,
    /// 匹配的文本（已脱敏）
    pub matched_text: String,
    /// 在原始文本中的位置
    pub position: usize,
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            safe: true,
            threats: Vec::new(),
            scan_duration_ms: 0,
        }
    }
}
