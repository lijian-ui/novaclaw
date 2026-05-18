//! 威胁模式定义
//! 
//! 定义 Prompt Injection 攻击的各种模式和检测规则

use serde::{Deserialize, Serialize};

/// 威胁严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreatSeverity {
    /// 低风险 - 需要关注但不一定阻止
    Low,
    /// 中风险 - 需要警告
    Medium,
    /// 高风险 - 应该阻止
    High,
    /// 严重风险 - 必须阻止
    Critical,
}

impl ThreatSeverity {
    /// 是否应该阻止
    pub fn should_block(&self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

/// 威胁模式定义
#[derive(Debug, Clone)]
pub struct ThreatPattern {
    /// 模式 ID
    pub id: &'static str,
    /// 正则表达式模式
    pub pattern: &'static str,
    /// 威胁类型
    pub threat_type: &'static str,
    /// 威胁描述
    pub description: &'static str,
    /// 严重程度
    pub severity: ThreatSeverity,
    /// 是否区分大小写
    pub case_insensitive: bool,
}

impl ThreatPattern {
    /// 创建新威胁模式
    pub const fn new(
        id: &'static str,
        pattern: &'static str,
        threat_type: &'static str,
        description: &'static str,
        severity: ThreatSeverity,
    ) -> Self {
        Self {
            id,
            pattern,
            threat_type,
            description,
            severity,
            case_insensitive: true,
        }
    }
}

/// 所有已知的威胁模式
pub fn get_all_threat_patterns() -> Vec<ThreatPattern> {
    vec![
        // ===== 指令覆盖类 =====
        ThreatPattern::new(
            "ignore_instructions",
            r"ignore\s+(previous|all|above|prior)\s+instructions",
            "prompt_injection",
            "尝试忽略之前的指令",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "disregard_rules",
            r"disregard\s+(your|all|any)\s+(instructions|rules|guidelines)",
            "prompt_injection",
            "尝试忽略所有规则",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "forget_prompt",
            r"(forget|ignore|clear)\s+(everything|all|your)\s+(previous|prior|past)\s+(instructions|context|conversation|prompt)",
            "prompt_injection",
            "尝试忘记之前的上下文",
            ThreatSeverity::High,
        ),
        
        // ===== 系统提示词泄露类 =====
        ThreatPattern::new(
            "sys_prompt_override",
            r"system\s+(prompt|instruction)\s*(override|leak|reveal|show)",
            "sys_prompt_leak",
            "尝试覆盖或泄露系统提示词",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "reveal_instructions",
            r"(reveal|show|display|print|tell).*(system|your).*(instructions|prompt|guidelines|rules)",
            "sys_prompt_leak",
            "尝试获取系统指令",
            ThreatSeverity::High,
        ),
        ThreatPattern::new(
            "extract_prompt",
            r"(extract|copy|repeat).*(your|system).*(instructions|prompt)",
            "sys_prompt_leak",
            "尝试提取系统提示词",
            ThreatSeverity::High,
        ),
        
        // ===== 角色扮演/越狱类 =====
        ThreatPattern::new(
            "jailbreak",
            r"(jailbreak|bypass|override|developer mode|do anything now|DAN)",
            "jailbreak",
            "越狱尝试",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "role_play_override",
            r"(act|pretend|role.?play)\s+as\s+(if|though)\s+(you|an?)\s+(have|are).*(no|without).*(restrictions|limits|rules|boundaries)",
            "jailbreak",
            "角色扮演绕过限制",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "new_instructions",
            r"new\s+(system\s+)?instructions?:",
            "prompt_injection",
            "尝试注入新指令",
            ThreatSeverity::High,
        ),
        
        // ===== 隐藏内容类 =====
        ThreatPattern::new(
            "html_comment_injection",
            r"<!--[^>]*?(?:ignore|override|system|secret|hidden|bypass)[^>]*?>",
            "hidden_content",
            "HTML 注释中的隐藏指令",
            ThreatSeverity::Medium,
        ),
        ThreatPattern::new(
            "hidden_div",
            r#"<\s*div\s+[^>]*style\s*=\s*["\'][^"\']*display\s*:\s*none[^"\']*["\'][^>]*>"#,
            "hidden_content",
            "隐藏的 HTML 元素",
            ThreatSeverity::Medium,
        ),
        ThreatPattern::new(
            "base64_injection",
            r#"(base64|decode|decrypt)\s*[:=]\s*['"][A-Za-z0-9+/]{20,}={0,2}['"]"#,
            "encoded_content",
            "Base64 编码的隐藏内容",
            ThreatSeverity::High,
        ),
        ThreatPattern::new(
            "hex_injection",
            r#"(hex|decode)\s*[:=]\s*['"]\\x[A-Fa-f0-9]+['"]"#,
            "encoded_content",
            "十六进制编码的隐藏内容",
            ThreatSeverity::Medium,
        ),
        
        // ===== 敏感操作类 =====
        ThreatPattern::new(
            "exfil_curl",
            r"curl\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|API|PASS)",
            "data_exfiltration",
            "尝试通过 curl 窃取敏感信息",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "read_secrets",
            r"(cat|read|type)\s+[^\n]*(\.env|credentials|\.netrc|\.pgpass|\.aws|\.npmrc|config\.json)",
            "credential_access",
            "尝试读取凭证文件",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "sql_injection",
            r"(union\s+select|drop\s+table|delete\s+from|insert\s+into)\s+",
            "injection_attack",
            "SQL 注入尝试",
            ThreatSeverity::Critical,
        ),
        ThreatPattern::new(
            "command_injection",
            r"[;&|`$]\s*(rm\s+-rf|del\s+/f|shutdown|init\s+0)",
            "command_injection",
            "命令注入尝试",
            ThreatSeverity::Critical,
        ),
        
        // ===== 欺骗类 =====
        ThreatPattern::new(
            "hide_from_user",
            r"(do\s+not|don't)\s+(tell|show|reveal|inform)\s+the\s+user",
            "deception",
            "尝试对用户隐瞒信息",
            ThreatSeverity::High,
        ),
        ThreatPattern::new(
            "pretend_success",
            r"(pretend|act\s+like)\s+(you|we)\s+(succeeded?|completed|finished)",
            "deception",
            "欺骗用户任务已成功",
            ThreatSeverity::Medium,
        ),
        
        // ===== 翻译执行类 =====
        ThreatPattern::new(
            "translate_execute",
            r"translate\s+[^\n]+\s+into\s+[^\n]+\s+and\s+(execute|run|eval|perform)",
            "indirect_execution",
            "翻译后执行攻击",
            ThreatSeverity::High,
        ),
        
        // ===== 特殊字符类 =====
        ThreatPattern::new(
            "zero_width_space",
            "[\u{200b}\u{200c}\u{200d}\u{200e}\u{200f}]",
            "invisible_characters",
            "零宽字符",
            ThreatSeverity::Low,
        ),
        ThreatPattern::new(
            "bom_marks",
            "[\u{feff}\u{fffe}\u{ffff}]",
            "invisible_characters",
            "BOM 标记字符",
            ThreatSeverity::Low,
        ),
        ThreatPattern::new(
            "rtl_override",
            "[\u{202a}-\u{202e}]",
            "invisible_characters",
            "文本方向覆盖字符",
            ThreatSeverity::Medium,
        ),
    ]
}
