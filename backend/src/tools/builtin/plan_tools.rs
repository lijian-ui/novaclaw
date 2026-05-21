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
            description:
                "提交一份执行计划给用户审批。当你面对复杂任务（3个以上步骤或有风险的操作）时，应该先调用此工具列清计划，等待用户确认后再执行。\n\n使用场景:\n- 用户要求做重大修改时\n- 涉及多文件、多步骤的操作\n- 有高风险变更（删除、重构、改数据库等）\n\n提交后请等待用户确认，不要直接开始执行。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "计划的目标概述"
                    },
                    "steps": {
                        "type": "array",
                        "description": "执行步骤列表",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {
                                    "type": "string",
                                    "description": "步骤标题"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "步骤详细描述"
                                },
                                "risk": {
                                    "type": "string",
                                    "enum": ["low", "med", "high"],
                                    "description": "风险等级: low=低风险, med=中风险(可能影响现有功能), high=高风险(可能破坏系统)"
                                }
                            },
                            "required": ["title", "description"]
                        }
                    },
                    "summary": {
                        "type": "string",
                        "description": "计划总结，说明整体影响和预期结果"
                    }
                },
                "required": ["goal", "steps", "summary"]
            }),
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
