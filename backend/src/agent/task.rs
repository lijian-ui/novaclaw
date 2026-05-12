use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub priority: u32,
    pub tool_name: Option<String>,
    pub tool_arguments: Option<String>,
    pub reasoning: Option<String>,
    pub result: Option<String>,
    pub quality_score: Option<f64>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub attempts: u32,
    pub max_attempts: u32,
}

impl SubTask {
    pub fn new(description: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            parent_id: None,
            dependencies: Vec::new(),
            status: TaskStatus::Pending,
            priority: 5,
            tool_name: None,
            tool_arguments: None,
            reasoning: None,
            result: None,
            quality_score: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            attempts: 0,
            max_attempts: 3,
        }
    }

    pub fn is_ready(&self, completed_ids: &HashSet<String>) -> bool {
        self.dependencies
            .iter()
            .all(|dep_id| completed_ids.contains(dep_id))
    }

    pub fn mark_completed(&mut self, result: &str, quality_score: Option<f64>) {
        self.status = TaskStatus::Completed;
        self.result = Some(result.to_string());
        self.quality_score = quality_score;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    pub fn mark_failed(&mut self, error: &str) {
        self.status = TaskStatus::Failed;
        self.result = Some(error.to_string());
        self.attempts += 1;
    }

    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tasks: Vec<SubTask>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_count: usize,
    pub total_count: usize,
}

impl TaskPlan {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            tasks: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            completed_count: 0,
            total_count: 0,
        }
    }

    pub fn add_task(&mut self, task: SubTask) {
        self.tasks.push(task);
        self.total_count = self.tasks.len();
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn update_task_status(&mut self, task_id: &str, status: TaskStatus) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            let old_status = task.status.clone();
            task.status = status;
            
            if old_status != TaskStatus::Completed && task.status == TaskStatus::Completed {
                self.completed_count += 1;
            } else if old_status == TaskStatus::Completed && task.status != TaskStatus::Completed {
                self.completed_count = self.completed_count.saturating_sub(1);
            }
            
            self.updated_at = chrono::Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    pub fn get_ready_tasks(&self) -> Vec<&SubTask> {
        let completed_ids: HashSet<String> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .map(|t| t.id.clone())
            .collect();

        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending && t.is_ready(&completed_ids))
            .collect()
    }

    pub fn get_progress(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.completed_count as f64 / self.total_count as f64
        }
    }

    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|t| t.status == TaskStatus::Completed)
    }

    pub fn get_task_by_id(&self, task_id: &str) -> Option<&SubTask> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    pub fn get_task_by_id_mut(&mut self, task_id: &str) -> Option<&mut SubTask> {
        self.tasks.iter_mut().find(|t| t.id == task_id)
    }
}

pub struct TaskDAG {
    graph: HashMap<String, Vec<String>>,
    in_degree: HashMap<String, usize>,
    task_map: HashMap<String, SubTask>,
}

impl TaskDAG {
    pub fn new(tasks: &[SubTask]) -> Self {
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut task_map: HashMap<String, SubTask> = HashMap::new();

        for task in tasks {
            task_map.insert(task.id.clone(), task.clone());
            in_degree.insert(task.id.clone(), task.dependencies.len());
            graph.insert(task.id.clone(), Vec::new());
        }

        for task in tasks {
            for dep_id in &task.dependencies {
                if let Some(children) = graph.get_mut(dep_id) {
                    children.push(task.id.clone());
                }
            }
        }

        Self {
            graph,
            in_degree,
            task_map,
        }
    }

    pub fn topological_sort(&self) -> Vec<String> {
        let mut in_degree = self.in_degree.clone();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut result: Vec<String> = Vec::new();

        for (id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(id.clone());
            }
        }

        while let Some(id) = queue.pop_front() {
            result.push(id.clone());

            if let Some(children) = self.graph.get(&id) {
                for child_id in children {
                    if let Some(degree) = in_degree.get_mut(child_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(child_id.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.task_map.len() {
            tracing::warn!("DAG contains cycles, partial ordering only");
        }

        result
    }

    pub fn has_cycle(&self) -> bool {
        self.topological_sort().len() != self.task_map.len()
    }

    pub fn get_dependencies(&self, task_id: &str) -> Vec<&SubTask> {
        self.task_map
            .get(task_id)
            .map(|task| {
                task.dependencies
                    .iter()
                    .filter_map(|dep_id| self.task_map.get(dep_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_dependents(&self, task_id: &str) -> Vec<&SubTask> {
        self.graph
            .get(task_id)
            .map(|children| {
                children
                    .iter()
                    .filter_map(|child_id| self.task_map.get(child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_execution_order(&self) -> Vec<Vec<&SubTask>> {
        let mut in_degree = self.in_degree.clone();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut result: Vec<Vec<&SubTask>> = Vec::new();

        for (id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(id.clone());
            }
        }

        while !queue.is_empty() {
            let level_size = queue.len();
            let mut level: Vec<&SubTask> = Vec::new();

            for _ in 0..level_size {
                if let Some(id) = queue.pop_front() {
                    if let Some(task) = self.task_map.get(&id) {
                        level.push(task);
                    }

                    if let Some(children) = self.graph.get(&id) {
                        for child_id in children {
                            if let Some(degree) = in_degree.get_mut(child_id) {
                                *degree -= 1;
                                if *degree == 0 {
                                    queue.push_back(child_id.clone());
                                }
                            }
                        }
                    }
                }
            }

            if !level.is_empty() {
                result.push(level);
            }
        }

        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub plan_id: String,
    pub plan_name: String,
    pub completed_count: usize,
    pub total_count: usize,
    pub progress: f64,
    pub status: String,
    pub tasks: Vec<TaskProgressItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressItem {
    pub task_id: String,
    pub description: String,
    pub status: String,
    pub priority: u32,
    pub result: Option<String>,
    pub quality_score: Option<f64>,
    pub attempts: u32,
}

impl From<&SubTask> for TaskProgressItem {
    fn from(task: &SubTask) -> Self {
        Self {
            task_id: task.id.clone(),
            description: task.description.clone(),
            status: match task.status {
                TaskStatus::Pending => "pending",
                TaskStatus::InProgress => "in_progress",
                TaskStatus::Completed => "completed",
                TaskStatus::Failed => "failed",
                TaskStatus::Skipped => "skipped",
            }
            .to_string(),
            priority: task.priority,
            result: task.result.clone(),
            quality_score: task.quality_score,
            attempts: task.attempts,
        }
    }
}

impl From<&TaskPlan> for TaskProgress {
    fn from(plan: &TaskPlan) -> Self {
        Self {
            plan_id: plan.id.clone(),
            plan_name: plan.name.clone(),
            completed_count: plan.completed_count,
            total_count: plan.total_count,
            progress: plan.get_progress(),
            status: if plan.is_complete() {
                "completed"
            } else {
                "in_progress"
            }
            .to_string(),
            tasks: plan.tasks.iter().map(TaskProgressItem::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDecompositionResult {
    pub plan: TaskPlan,
    pub execution_order: Vec<Vec<String>>,
    pub has_cycles: bool,
}
