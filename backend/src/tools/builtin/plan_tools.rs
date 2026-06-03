use crate::tools::registry::{ToolDef, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// 计划的单个步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlanStep {
    title: String,
    description: String,
    #[serde(default = "default_risk")]
    risk: String, // "low" | "med" | "high"
}

fn default_risk() -> String {
    "low".to_string()
}

/// 持久化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlanDocument {
    goal: String,
    steps: Vec<PlanStep>,
    summary: String,
    status: String, // "pending" | "approved" | "rejected"
    created_at: String,
    updated_at: String,
}

/// 获取 plan 文件路径
fn plan_path(session_id: &str) -> PathBuf {
    crate::config::get_base_dir()
        .join("plans")
        .join(format!("{}.json", session_id))
}

/// 保存计划
fn save_plan(session_id: &str, plan: &PlanDocument) -> Result<(), String> {
    let path = plan_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 plans 目录失败: {}", e))?;
    }
    let json =
        serde_json::to_string_pretty(plan).map_err(|e| format!("序列化计划失败: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("写入计划文件失败: {}", e))?;
    Ok(())
}

/// 格式化计划为可读文本
fn format_plan(plan: &PlanDocument) -> String {
    let mut lines = vec![format!("# 📋 执行计划\n"), format!("**目标**: {}\n", plan.goal)];
    for (i, step) in plan.steps.iter().enumerate() {
        let risk_tag = match step.risk.as_str() {
            "high" => " ⚠️高风险",
            "med" => " 🔶中风险",
            _ => "",
        };
        lines.push(format!("### {}. {}{}", i + 1, step.title, risk_tag));
        lines.push(format!("   {}", step.description));
        lines.push(String::new());
    }
    lines.push(format!("---\n**总结**: {}", plan.summary));
    lines.push(format!("\n*状态: {}*", match plan.status.as_str() {
        "pending" => "等待审批中，请用户确认后继续",
        "approved" => "已批准，可以开始执行",
        "rejected" => "已拒绝",
        _ => &plan.status,
    }));
    lines.join("\n")
}

/// 注册 plan 工具: submit_plan
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
                        name: "submit_plan".to_string(),
            display_name: "提交计划".to_string(),
            description:
                "Submit an execution plan for user approval. When facing complex tasks (3+ steps or risky operations), call this tool to lay out the plan first, then wait for user confirmation before executing.\n\nUse cases:\n- Major changes requested by user\n- Multi-file, multi-step operations\n- High-risk changes (delete, refactor, database changes, etc.)\n\nAfter submitting, wait for user approval before starting execution.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "Overview of the plan's goal"
                    },
                    "steps": {
                        "type": "array",
                        "description": "List of execution steps",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {
                                    "type": "string",
                                    "description": "Step title"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Detailed step description"
                                },
                                "risk": {
                                    "type": "string",
                                    "enum": ["low", "med", "high"],
                                    "description": "Risk level: low (safe), med (may affect existing functionality), high (may break the system)"
                                }
                            },
                            "required": ["title", "description"]
                        }
                    },
                    "summary": {
                        "type": "string",
                        "description": "Plan summary describing overall impact and expected outcomes"
                    }
                },
                "required": ["goal", "steps", "summary"]
            }),
            skip_truncation_save: false,
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let goal = args["goal"]
                        .as_str()
                        .ok_or("Missing 'goal' parameter")?
                        .to_string();
                    let summary = args["summary"]
                        .as_str()
                        .ok_or("Missing 'summary' parameter")?
                        .to_string();
                    let steps_val = args
                        .get("steps")
                        .and_then(|v| v.as_array())
                        .ok_or("Missing 'steps' parameter")?;

                    let steps: Vec<PlanStep> =
                        serde_json::from_value(serde_json::Value::Array(steps_val.clone()))
                            .map_err(|e| format!("参数格式错误: {}", e))?;

                    if steps.is_empty() {
                        return Err("步骤列表不能为空".to_string());
                    }

                    let now = chrono::Utc::now().to_rfc3339();
                    let plan = PlanDocument {
                        goal: goal.clone(),
                        steps,
                        summary: summary.clone(),
                        status: "pending".to_string(),
                        created_at: now.clone(),
                        updated_at: now,
                    };

                    let session_id = args["_session_id"].as_str().unwrap_or("default");
                    save_plan(session_id, &plan)?;

                    let formatted = format_plan(&plan);
                    Ok(format!(
                        "{}\n\n---\n请用户审阅上述计划，确认后回复「继续」或提供修改意见。",
                        formatted
                    ))
                },
            ),
        })
        .await;
}
