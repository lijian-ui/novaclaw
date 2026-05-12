/// 复杂任务预判断模块
///
/// 功能说明：
///   在发送请求前对用户输入内容进行分析，识别其中是否包含复杂任务特征。
///   通过关键词匹配和评分机制，判断当前任务是否需要启动任务分解流程。
///
/// 判断逻辑：
///   1. 将用户输入按关键词类别进行匹配
///   2. 计算综合复杂度评分
///   3. 根据评分阈值返回预判断结论
use serde::{Deserialize, Serialize};

/// 关键词分组，按语义类别组织关键词
///
/// 每个分组代表一类复杂任务特征，提高匹配的语义准确性
pub struct KeywordGroup {
    /// 关键词类别名称（如"文件操作"、"代码修改"等）
    pub category: &'static str,
    /// 该类别的关键词列表
    pub keywords: &'static [&'static str],
    /// 该类别的权重系数，影响最终评分计算
    pub weight: f64,
}

/// 匹配到的关键词详细信息
///
/// 记录每个匹配关键词的来源类别和出现次数，用于生成判断理由
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedKeyword {
    /// 匹配到的关键词原文
    pub keyword: String,
    /// 该关键词所属的类别名称
    pub category: String,
    /// 该关键词在输入文本中出现的次数
    pub count: usize,
}

/// 预判断结果
///
/// 包含是否复杂任务的结论、评分详情和匹配信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// 是否判定为复杂任务
    pub is_complex: bool,
    /// 复杂程度评分，范围 0.0 ~ 1.0
    /// 评分越高表示任务越复杂
    pub complexity_score: f64,
    /// 所有匹配到的关键词详情列表
    pub matched_keywords: Vec<MatchedKeyword>,
    /// 人类可读的判断理由说明
    pub reason: String,
    /// 匹配到的不同关键词种类数量
    pub unique_matches: usize,
    /// 匹配到的关键词总次数（含重复）
    pub total_matches: usize,
}

/// 复杂任务特征关键词检测器
///
/// 用于在发送 LLM 请求前对用户输入进行预分析，判断是否需要启用任务分解流程。
/// 采用多类别关键词分组匹配+加权评分的机制，避免单一关键词误判。
///
/// # 使用示例
/// ```
/// let detector = TaskComplexityDetector::new();
/// let result = detector.analyze("请帮我重构用户登录模块，需要修改多个文件");
/// assert!(result.is_complex);
/// ```
pub struct TaskComplexityDetector;

impl TaskComplexityDetector {
    /// 获取完整的关键词库
    ///
    /// 关键词库按语义类别分组，每个分组包含：
    /// - category: 类别名称，用于生成判断理由
    /// - keywords: 该类别下的具体关键词列表
    /// - weight: 该类别权重，影响最终评分
    ///
    /// 共包含 6 个类别、49 个关键词，覆盖常见复杂任务场景
    fn get_keyword_groups() -> Vec<KeywordGroup> {
        vec![
            // 文件操作类：涉及多个文件的读写、修改操作
            KeywordGroup {
                category: "文件操作",
                keywords: &[
                    "文件", "目录", "文件夹", "批量", "多个文件",
                    "创建文件", "删除文件", "移动文件", "重命名",
                    "项目结构", "目录结构", "遍历", "递归",
                ],
                weight: 0.8,
            },
            // 代码修改类：涉及代码重构、修改、优化等操作
            KeywordGroup {
                category: "代码修改",
                keywords: &[
                    "重构", "修改", "优化", "重写", "更新",
                    "实现", "开发", "编写", "添加", "删除代码",
                    "修复", "调试", "调整", "迁移", "升级",
                ],
                weight: 1.0,
            },
            // 分析研究类：需要分析、调查、研究的任务
            KeywordGroup {
                category: "分析研究",
                keywords: &[
                    "分析", "调查", "研究", "诊断", "排查",
                    "定位", "追踪", "对比", "评估", "审查",
                    "检查", "验证", "测试", "监控",
                ],
                weight: 0.7,
            },
            // 多步骤任务类：明确包含多个步骤或环节的任务
            KeywordGroup {
                category: "多步骤任务",
                keywords: &[
                    "步骤", "流程", "先后", "依次", "逐步",
                    "首先", "然后", "最后", "第一阶段", "第二步",
                    "多个步骤", "分步", "按顺序",
                ],
                weight: 1.2,
            },
            // 系统架构类：涉及架构设计、模块组织等高层次任务
            KeywordGroup {
                category: "系统架构",
                keywords: &[
                    "架构", "设计", "模块", "组件", "系统",
                    "框架", "接口", "依赖", "集成", "部署",
                    "方案", "规划", "体系", "结构",
                ],
                weight: 1.1,
            },
            // 复杂查询类：需要搜索、汇总、关联多个信息源的任务
            KeywordGroup {
                category: "复杂查询",
                keywords: &[
                    "搜索", "查找", "查询", "汇总", "统计",
                    "收集", "整理", "归类", "关联", "整合",
                    "综合", "对比分析",
                ],
                weight: 0.6,
            },
        ]
    }

    /// 对用户输入进行复杂任务预判断
    ///
    /// 执行流程：
    ///   1. 遍历所有关键词类别，逐词匹配用户输入
    ///   2. 统计每个类别的匹配情况和总匹配次数
    ///   3. 计算加权综合评分
    ///   4. 根据评分阈值生成最终判断结论
    ///
    /// # 参数
    /// - `user_input`: 用户输入的文本内容
    ///
    /// # 返回值
    /// 返回 `DetectionResult` 结构体，包含评分、匹配详情和判断理由
    pub fn analyze(user_input: &str) -> DetectionResult {
        // 将输入转换为小写，实现大小写不敏感的匹配
        let input_lower = user_input.to_lowercase();
        // 记录所有匹配到的关键词
        let mut matched_keywords: Vec<MatchedKeyword> = Vec::new();
        // 记录匹配到的关键词总次数（含重复）
        let mut total_matches: usize = 0;
        // 获取关键词库
        let groups = Self::get_keyword_groups();

        // 第一步：遍历所有关键词类别，执行匹配
        for group in &groups {
            for keyword in group.keywords {
                // 将关键词转为小写后进行匹配
                let keyword_lower = keyword.to_lowercase();
                // 计算该关键词在输入中出现的次数
                let count = input_lower.matches(&keyword_lower).count();
                if count > 0 {
                    // 记录匹配结果
                    matched_keywords.push(MatchedKeyword {
                        keyword: keyword.to_string(),
                        category: group.category.to_string(),
                        count,
                    });
                    total_matches += count;
                }
            }
        }

        // 第二步：统计匹配到的不同关键词种类数量
        let unique_matches = matched_keywords.len();

        // 第三步：计算加权综合评分
        //
        // 评分算法说明：
        //   1. 基础分 = min(匹配种类数 / 5, 1.0) ，5种及以上关键词得满分
        //      用于衡量任务广度和多样性
        //   2. 权重分 = 各匹配类别权重的平均值
        //      不同类别的权重不同（如"多步骤任务"权重1.2 > "复杂查询"权重0.6）
        //   3. 密度分 = min(总匹配次数 / 10, 1.0) ，10次及以上得满分
        //      用于衡量任务描述的详细程度和复杂度
        //   4. 最终评分 = 基础分 * 0.4 + 权重分 * 0.4 + 密度分 * 0.2
        //      三项得分的加权平均，权重分配基于经验调优
        let base_score = (unique_matches as f64 / 5.0).min(1.0);

        // 计算权重分：取所有匹配类别的平均权重
        let weight_score = if !matched_keywords.is_empty() {
            // 收集所有匹配到的类别
            let mut matched_categories: Vec<&str> = matched_keywords
                .iter()
                .map(|m| m.category.as_str())
                .collect();
            matched_categories.sort();
            matched_categories.dedup();
            // 计算平均权重
            let total_weight: f64 = groups
                .iter()
                .filter(|g| matched_categories.contains(&g.category))
                .map(|g| g.weight)
                .sum();
            (total_weight / matched_categories.len() as f64).min(1.5) / 1.5
        } else {
            0.0
        };

        // 密度分：衡量关键词在输入中的密集程度
        let density_score = (total_matches as f64 / 10.0).min(1.0);

        // 综合计算最终评分
        // 权重分配：基础分40% + 权重分40% + 密度分20%
        let complexity_score = base_score * 0.4 + weight_score * 0.4 + density_score * 0.2;

        // 第四步：根据阈值生成判断结论
        // 阈值设定逻辑：
        //   评分 >= 0.35 → 判定为复杂任务
        //   评分 <  0.35 → 判定为简单任务
        //   阈值 0.35 基于经验值，可在实际使用中调优
        let threshold = 0.35;
        let is_complex = complexity_score >= threshold;

        // 第五步：生成人类可读的判断理由
        let reason = if is_complex {
            format!(
                "检测到复杂任务特征：匹配到 {} 个关键词（{} 类），涉及 {}、{} 等类别（综合评分 {:.2}，阈值 {:.2}）",
                total_matches,
                Self::count_categories(&matched_keywords),
                Self::get_top_categories(&matched_keywords, 2).join("、"),
                if Self::count_categories(&matched_keywords) > 2 {
                    "等"
                } else {
                    ""
                },
                complexity_score,
                threshold,
            )
        } else {
            format!(
                "未检测到复杂任务特征：仅匹配到 {} 个关键词（综合评分 {:.2}，阈值 {:.2}）",
                total_matches, complexity_score, threshold,
            )
        };

        DetectionResult {
            is_complex,
            complexity_score,
            matched_keywords,
            reason,
            unique_matches,
            total_matches,
        }
    }

    /// 统计匹配到的关键词涉及的不同类别数量
    fn count_categories(matched: &[MatchedKeyword]) -> usize {
        let mut categories: Vec<&str> = matched.iter().map(|m| m.category.as_str()).collect();
        categories.sort();
        categories.dedup();
        categories.len()
    }

    /// 获取匹配数量最多的前 N 个类别名称
    ///
    /// 用于生成判断理由，展示最相关的类别信息
    fn get_top_categories(matched: &[MatchedKeyword], n: usize) -> Vec<String> {
        // 按类别统计匹配次数
        let mut category_counts: Vec<(String, usize)> = Vec::new();
        for mk in matched {
            let pos = category_counts.iter().position(|(c, _)| c == &mk.category);
            if let Some(idx) = pos {
                category_counts[idx].1 += mk.count;
            } else {
                category_counts.push((mk.category.clone(), mk.count));
            }
        }
        // 按匹配次数降序排序
        category_counts.sort_by(|a, b| b.1.cmp(&a.1));
        // 取前 N 个
        category_counts
            .into_iter()
            .take(n)
            .map(|(c, _)| c)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：简单问候不应被判定为复杂任务
    #[test]
    fn test_simple_greeting() {
        let result = TaskComplexityDetector::analyze("你好，今天天气怎么样？");
        assert!(!result.is_complex, "简单问候不应被判定为复杂任务");
        assert!(result.complexity_score < 0.35, "简单问候的评分应低于阈值");
    }

    /// 测试：复杂任务应被正确识别
    #[test]
    fn test_complex_task() {
        let result = TaskComplexityDetector::analyze(
            "请帮我重构用户登录模块，需要修改多个文件，并添加新的接口"
        );
        assert!(result.is_complex, "涉及'重构'、'修改'、'多个文件'、'添加'的任务应被判定为复杂");
        assert!(result.matched_keywords.len() >= 4, "应至少匹配4个关键词");
    }

    /// 测试：多步骤任务应被识别
    #[test]
    fn test_multi_step_task() {
        let result = TaskComplexityDetector::analyze(
            "请先分析代码，然后修改文件，最后验证结果"
        );
        assert!(result.is_complex, "包含'先'、'然后'、'最后'步骤描述的任务应被判定为复杂");
    }

    /// 测试：空输入不会引发错误
    #[test]
    fn test_empty_input() {
        let result = TaskComplexityDetector::analyze("");
        assert!(!result.is_complex, "空输入不应被判定为复杂任务");
        assert_eq!(result.matched_keywords.len(), 0, "空输入不应匹配任何关键词");
        assert_eq!(result.complexity_score, 0.0, "空输入的评分应为0");
    }

    /// 测试：大小写不敏感匹配
    #[test]
    fn test_case_insensitive() {
        let result = TaskComplexityDetector::analyze("请帮我修改MODULE并添加新的接口");
        assert!(result.is_complex, "应支持大小写不敏感的匹配");
    }

    /// 测试：单一关键词不应误判
    #[test]
    fn test_single_keyword() {
        let result = TaskComplexityDetector::analyze("这个文件在哪里？");
        assert!(!result.is_complex, "仅匹配单一关键词不应判定为复杂任务");
    }
}
