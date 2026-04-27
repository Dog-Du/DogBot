use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    pub task_id: String,
    pub conversation_key: String,
    pub actor_id: String,
    pub trigger_message_id: Option<String>,
}

impl TaskSummary {
    pub fn new(task_id: impl Into<String>, conversation_key: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            conversation_key: conversation_key.into(),
            actor_id: String::new(),
            trigger_message_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    StartNow,
    Queued { tasks_ahead: usize },
    QueueFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalState {
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSummary {
    pub task_id: String,
    pub conversation_key: String,
    pub state: TerminalState,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerSnapshot {
    pub active_count: usize,
    pub max_concurrent: usize,
    pub waiting_count: usize,
    pub max_queue_depth: usize,
    pub running: Vec<TaskSummary>,
    pub recent_terminal: Vec<TerminalSummary>,
}

#[derive(Debug)]
pub struct SchedulerState {
    max_concurrent: usize,
    max_queue_depth: usize,
    running: HashMap<String, TaskSummary>,
    queued: HashMap<String, VecDeque<TaskSummary>>,
    ready_conversations: VecDeque<String>,
    recent_terminal: VecDeque<TerminalSummary>,
}

impl SchedulerState {
    const TERMINAL_HISTORY_CAPACITY: usize = 16;

    pub fn new(max_concurrent: usize, max_queue_depth: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            max_queue_depth,
            running: HashMap::new(),
            queued: HashMap::new(),
            ready_conversations: VecDeque::new(),
            recent_terminal: VecDeque::new(),
        }
    }

    pub fn admit(&mut self, task: TaskSummary) -> Admission {
        if self.can_start_now(&task.conversation_key) {
            self.running.insert(task.conversation_key.clone(), task);
            return Admission::StartNow;
        }

        if self.waiting_count() >= self.max_queue_depth {
            return Admission::QueueFull;
        }

        let tasks_ahead = self.tasks_ahead_for(&task.conversation_key);
        let conversation_key = task.conversation_key.clone();
        let was_empty = self
            .queued
            .get(&conversation_key)
            .map(VecDeque::is_empty)
            .unwrap_or(true);

        self.queued
            .entry(conversation_key.clone())
            .or_default()
            .push_back(task);

        if was_empty && !self.running.contains_key(&conversation_key) {
            self.push_ready_conversation(conversation_key);
        }

        Admission::Queued { tasks_ahead }
    }

    pub fn finish(
        &mut self,
        conversation_key: &str,
        terminal: TerminalState,
        summary: Option<String>,
    ) -> Vec<TaskSummary> {
        if let Some(finished) = self.running.remove(conversation_key) {
            self.push_terminal(TerminalSummary {
                task_id: finished.task_id,
                conversation_key: finished.conversation_key,
                state: terminal,
                summary,
            });
        }

        let mut promoted = Vec::new();
        if let Some(next) = self.pop_next_for_conversation(conversation_key) {
            self.running
                .insert(next.conversation_key.clone(), next.clone());
            promoted.push(next);
        }

        while self.running.len() < self.max_concurrent {
            let Some(conversation) = self.pop_ready_conversation() else {
                break;
            };
            if self.running.contains_key(&conversation) {
                continue;
            }
            let Some(next) = self.pop_next_for_conversation(&conversation) else {
                continue;
            };
            self.running
                .insert(next.conversation_key.clone(), next.clone());
            promoted.push(next);
        }

        promoted
    }

    pub fn snapshot(&self) -> SchedulerSnapshot {
        let mut running = self.running.values().cloned().collect::<Vec<_>>();
        running.sort_by(|left, right| left.conversation_key.cmp(&right.conversation_key));

        SchedulerSnapshot {
            active_count: self.running.len(),
            max_concurrent: self.max_concurrent,
            waiting_count: self.waiting_count(),
            max_queue_depth: self.max_queue_depth,
            running,
            recent_terminal: self.recent_terminal.iter().cloned().collect(),
        }
    }

    fn can_start_now(&self, conversation_key: &str) -> bool {
        self.running.len() < self.max_concurrent
            && !self.running.contains_key(conversation_key)
            && self
                .queued
                .get(conversation_key)
                .map(VecDeque::is_empty)
                .unwrap_or(true)
    }

    fn tasks_ahead_for(&self, conversation_key: &str) -> usize {
        if self.running.contains_key(conversation_key) || self.queued.contains_key(conversation_key)
        {
            usize::from(self.running.contains_key(conversation_key))
                + self
                    .queued
                    .get(conversation_key)
                    .map(VecDeque::len)
                    .unwrap_or(0)
        } else {
            self.running.len() + self.waiting_count()
        }
    }

    fn waiting_count(&self) -> usize {
        self.queued.values().map(VecDeque::len).sum()
    }

    fn push_ready_conversation(&mut self, conversation_key: String) {
        if !self
            .ready_conversations
            .iter()
            .any(|key| key == &conversation_key)
        {
            self.ready_conversations.push_back(conversation_key);
        }
    }

    fn pop_ready_conversation(&mut self) -> Option<String> {
        self.ready_conversations.pop_front()
    }

    fn pop_next_for_conversation(&mut self, conversation_key: &str) -> Option<TaskSummary> {
        let next = self.queued.get_mut(conversation_key)?.pop_front();
        if self
            .queued
            .get(conversation_key)
            .map(VecDeque::is_empty)
            .unwrap_or(false)
        {
            self.queued.remove(conversation_key);
        }
        next
    }

    fn push_terminal(&mut self, summary: TerminalSummary) {
        self.recent_terminal.push_back(summary);
        if self.recent_terminal.len() > Self::TERMINAL_HISTORY_CAPACITY {
            self.recent_terminal.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_immediately_when_capacity_and_conversation_are_free() {
        let mut scheduler = SchedulerState::new(2, 8);
        let admission = scheduler.admit(TaskSummary::new("t1", "qq:group:1"));
        assert_eq!(admission, Admission::StartNow);
        assert_eq!(scheduler.snapshot().active_count, 1);
    }

    #[test]
    fn queues_behind_running_same_conversation() {
        let mut scheduler = SchedulerState::new(2, 8);
        assert_eq!(
            scheduler.admit(TaskSummary::new("t1", "qq:group:1")),
            Admission::StartNow
        );
        assert_eq!(
            scheduler.admit(TaskSummary::new("t2", "qq:group:1")),
            Admission::Queued { tasks_ahead: 1 }
        );
        assert_eq!(scheduler.snapshot().waiting_count, 1);
    }

    #[test]
    fn queues_when_global_capacity_is_full() {
        let mut scheduler = SchedulerState::new(1, 8);
        assert_eq!(
            scheduler.admit(TaskSummary::new("t1", "qq:group:1")),
            Admission::StartNow
        );
        assert_eq!(
            scheduler.admit(TaskSummary::new("t2", "qq:group:2")),
            Admission::Queued { tasks_ahead: 1 }
        );
    }

    #[test]
    fn queue_capacity_rejects_waiting_task() {
        let mut scheduler = SchedulerState::new(1, 1);
        assert_eq!(
            scheduler.admit(TaskSummary::new("t1", "qq:group:1")),
            Admission::StartNow
        );
        assert_eq!(
            scheduler.admit(TaskSummary::new("t2", "qq:group:2")),
            Admission::Queued { tasks_ahead: 1 }
        );
        assert_eq!(
            scheduler.admit(TaskSummary::new("t3", "qq:group:3")),
            Admission::QueueFull
        );
    }

    #[test]
    fn completion_promotes_same_conversation_then_ready_conversations() {
        let mut scheduler = SchedulerState::new(2, 8);
        scheduler.admit(TaskSummary::new("a1", "qq:group:1"));
        scheduler.admit(TaskSummary::new("a2", "qq:group:1"));
        scheduler.admit(TaskSummary::new("b1", "qq:group:2"));
        let promoted = scheduler.finish("qq:group:1", TerminalState::Completed, None);
        assert_eq!(
            promoted
                .iter()
                .map(|task| task.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a2"]
        );
    }

    #[test]
    fn finishing_one_task_fills_remaining_global_capacity() {
        let mut scheduler = SchedulerState::new(1, 8);
        scheduler.admit(TaskSummary::new("a1", "qq:group:1"));
        scheduler.admit(TaskSummary::new("b1", "qq:group:2"));
        scheduler.admit(TaskSummary::new("c1", "qq:group:3"));

        let promoted = scheduler.finish("qq:group:1", TerminalState::Completed, None);
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].task_id, "b1");
        assert_eq!(scheduler.snapshot().waiting_count, 1);
    }

    #[test]
    fn queued_new_conversation_counts_all_waiting_tasks_ahead() {
        let mut scheduler = SchedulerState::new(1, 8);
        scheduler.admit(TaskSummary::new("a1", "qq:group:1"));
        scheduler.admit(TaskSummary::new("b1", "qq:group:2"));
        scheduler.admit(TaskSummary::new("b2", "qq:group:2"));

        assert_eq!(
            scheduler.admit(TaskSummary::new("c1", "qq:group:3")),
            Admission::Queued { tasks_ahead: 3 }
        );
    }
}
