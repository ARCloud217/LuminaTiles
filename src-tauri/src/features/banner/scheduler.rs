//! 横幅调度核心：池、队列、占比、挤占、计时都在这里，只做纯计算。
//! 时钟由调用方传入，不碰窗口与事件，因此可以逐条覆盖规则。

use std::time::{Duration, Instant};

use crate::features::messages::Message;

/// 同屏最多几条。
pub const SLOT_LIMIT: usize = 3;
/// 等待队列上限；再满就按用户要求销毁屏上最早入场的一条。
pub const QUEUE_LIMIT: usize = 20;

#[derive(Clone, Debug)]
pub struct Slot {
    pub message: Message,
    /// 占「去掉间距后的剩余宽度」的百分比
    pub width: f64,
    pub deadline: Instant,
    /// 入场序号：同级先入先出，也是「最早显示」的依据
    pub seq: u64,
}

#[derive(Clone, Debug)]
struct Queued {
    message: Message,
    seq: u64,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub slots: Vec<Slot>,
    pub queue_count: usize,
    pub queue_head: Option<String>,
}

pub struct Scheduler {
    slots: Vec<Slot>,
    queue: VecDeque<Queued>,
    seq: u64,
    default_duration_ms: u64,
}

use std::collections::VecDeque;

impl Scheduler {
    pub fn new(default_duration_ms: u64) -> Self {
        Self {
            slots: Vec::new(),
            queue: VecDeque::new(),
            seq: 0,
            default_duration_ms,
        }
    }

    /// 横幅设置里的默认停留时长；已排队的消息不受影响，只影响之后上屏的。
    pub fn set_default_duration(&mut self, duration_ms: u64) {
        self.default_duration_ms = duration_ms;
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.queue.is_empty()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            slots: self.slots.clone(),
            queue_count: self.queue.len(),
            queue_head: self.queue.front().map(|queued| queued.message.id.clone()),
        }
    }

    /// 一条消息到达：就地更新 / 上屏 / 挤占 / 入队。返回是否有变化。
    pub fn arrive(&mut self, message: Message, now: Instant) -> bool {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.message.id == message.id)
        {
            let duration = self.duration_of(&message);
            self.slots[index].message = message;
            self.slots[index].deadline = now + duration;
            self.relayout();
            return true;
        }
        if let Some(index) = self
            .queue
            .iter()
            .position(|queued| queued.message.id == message.id)
        {
            self.queue[index].message = message;
            self.sort_queue();
            return true;
        }
        if self.slots.len() < SLOT_LIMIT {
            self.insert_slot(message, now);
            return true;
        }
        let lowest = self
            .slots
            .iter()
            .map(|slot| slot.message.importance.rank())
            .min()
            .unwrap_or(0);
        if message.importance.rank() > lowest {
            let victim = self.victim_index();
            self.slots.remove(victim);
            self.insert_slot(message, now);
            return true;
        }
        self.insert_queued(message);
        if self.queue.len() > QUEUE_LIMIT {
            // 队列满：销毁屏上最早入场的一条（不看等级），再按队列优先级补位。
            let oldest = self.oldest_slot_index();
            self.slots.remove(oldest);
            self.refill(now);
        }
        true
    }

    /// 到期移除并补位。返回是否有变化。
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut expired: Vec<(Instant, u64, String)> = self
            .slots
            .iter()
            .filter(|slot| slot.deadline <= now)
            .map(|slot| (slot.deadline, slot.seq, slot.message.id.clone()))
            .collect();
        if expired.is_empty() {
            return false;
        }
        expired.sort();
        for (_, _, id) in expired {
            self.slots.retain(|slot| slot.message.id != id);
        }
        self.refill(now);
        true
    }

    /// 撤销一条消息（点击、外部调用）。返回是否有变化。
    pub fn dismiss(&mut self, id: &str, now: Instant) -> bool {
        let slots_before = self.slots.len();
        self.slots.retain(|slot| slot.message.id != id);
        let removed_slot = self.slots.len() != slots_before;
        let queue_before = self.queue.len();
        self.queue.retain(|queued| queued.message.id != id);
        let removed_queue = self.queue.len() != queue_before;
        if removed_slot {
            self.refill(now);
        }
        removed_slot || removed_queue
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.queue.clear();
    }

    fn duration_of(&self, message: &Message) -> Duration {
        if message.duration_ms > 0 {
            Duration::from_millis(message.duration_ms)
        } else {
            Duration::from_millis(self.default_duration_ms)
        }
    }

    fn insert_slot(&mut self, message: Message, now: Instant) {
        self.seq += 1;
        let seq = self.seq;
        let deadline = now + self.duration_of(&message);
        self.slots.push(Slot {
            message,
            width: 0.0,
            deadline,
            seq,
        });
        self.relayout();
    }

    fn insert_queued(&mut self, message: Message) {
        self.seq += 1;
        let seq = self.seq;
        self.queue.push_back(Queued { message, seq });
        self.sort_queue();
    }

    fn sort_queue(&mut self) {
        self.queue.make_contiguous().sort_by(|a, b| {
            b.message
                .importance
                .rank()
                .cmp(&a.message.importance.rank())
                .then(a.seq.cmp(&b.seq))
        });
    }

    fn refill(&mut self, now: Instant) {
        while self.slots.len() < SLOT_LIMIT {
            let Some(queued) = self.queue.pop_front() else {
                break;
            };
            self.insert_slot(queued.message, now);
        }
        self.relayout();
    }

    /// 挤占受害者：最低等级里最早入场的那条。
    fn victim_index(&self) -> usize {
        let mut victim = 0;
        for (index, slot) in self.slots.iter().enumerate() {
            let current = (slot.message.importance.rank(), slot.seq);
            let best = (self.slots[victim].message.importance.rank(), self.slots[victim].seq);
            if current < best {
                victim = index;
            }
        }
        victim
    }

    fn oldest_slot_index(&self) -> usize {
        let mut oldest = 0;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.seq < self.slots[oldest].seq {
                oldest = index;
            }
        }
        oldest
    }

    /// 重新排列并分配宽度：等级降序、同级先入先出；3 格且最高等级唯一时居中。
    fn relayout(&mut self) {
        if self.slots.is_empty() {
            return;
        }
        let mut sorted = self.slots.clone();
        sorted.sort_by(|a, b| {
            b.message
                .importance
                .rank()
                .cmp(&a.message.importance.rank())
                .then(a.seq.cmp(&b.seq))
        });
        let widths = widths_for(&sorted);
        for (slot, width) in sorted.iter_mut().zip(widths) {
            slot.width = width;
        }
        if sorted.len() == SLOT_LIMIT
            && sorted[0].message.importance.rank() > sorted[1].message.importance.rank()
        {
            let top = sorted.remove(0);
            let left = sorted.remove(0);
            let right = sorted.remove(0);
            self.slots = vec![left, top, right];
        } else {
            self.slots = sorted;
        }
    }
}

/// 宽度表：输入按「等级降序、同级先入先出」排好的格子，返回同序的百分比。
fn widths_for(slots: &[Slot]) -> Vec<f64> {
    let ranks: Vec<u8> = slots
        .iter()
        .map(|slot| slot.message.importance.rank())
        .collect();
    match ranks.as_slice() {
        [_] => vec![100.0],
        [a, b] if a == b => vec![50.0, 50.0],
        [2, 1] => vec![60.0, 40.0],
        [2, 0] => vec![66.0, 34.0],
        [1, 0] => vec![58.0, 42.0],
        [a, b, c] if a == b && b == c => vec![33.3, 33.3, 33.4],
        [2, 2, 1] => vec![38.0, 38.0, 24.0],
        [2, 2, 0] => vec![40.0, 40.0, 20.0],
        [2, 1, 1] => vec![42.0, 29.0, 29.0],
        [2, 1, 0] => vec![50.0, 30.0, 20.0],
        [2, 0, 0] => vec![50.0, 25.0, 25.0],
        [1, 1, 0] => vec![38.0, 38.0, 24.0],
        [1, 0, 0] => vec![50.0, 25.0, 25.0],
        other => {
            let share = 100.0 / other.len().max(1) as f64;
            vec![share; other.len()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::messages::Importance;

    fn base() -> Instant {
        Instant::now()
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn message(id: &str, importance: Importance, duration_ms: u64) -> Message {
        Message {
            id: id.to_string(),
            level: "info".to_string(),
            importance,
            i18n_key: None,
            text: Some(id.to_string()),
            params: None,
            origin: "test".to_string(),
            duration_ms,
            count: 1,
        }
    }

    fn ids(scheduler: &Scheduler) -> Vec<String> {
        scheduler
            .snapshot()
            .slots
            .iter()
            .map(|slot| slot.message.id.clone())
            .collect()
    }

    fn widths(scheduler: &Scheduler) -> Vec<f64> {
        scheduler
            .snapshot()
            .slots
            .iter()
            .map(|slot| slot.width)
            .collect()
    }

    #[test]
    fn single_slot_takes_full_width() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("a", Importance::Normal, 1000), now);
        assert_eq!(widths(&scheduler), vec![100.0]);
    }

    #[test]
    fn two_slots_follow_table() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("h", Importance::High, 1000), now);
        scheduler.arrive(message("l", Importance::Low, 1000), now);
        assert_eq!(ids(&scheduler), vec!["h", "l"]);
        assert_eq!(widths(&scheduler), vec![66.0, 34.0]);

        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("n", Importance::Normal, 1000), now);
        scheduler.arrive(message("n2", Importance::Normal, 1000), now);
        assert_eq!(widths(&scheduler), vec![50.0, 50.0]);
    }

    #[test]
    fn one_high_two_low_centers_high() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("h", Importance::High, 1000), now);
        scheduler.arrive(message("l1", Importance::Low, 1000), now);
        scheduler.arrive(message("l2", Importance::Low, 1000), now);
        assert_eq!(ids(&scheduler), vec!["l1", "h", "l2"]);
        assert_eq!(widths(&scheduler), vec![25.0, 50.0, 25.0]);
    }

    #[test]
    fn one_high_two_normal_centers_high_with_more_room() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("h", Importance::High, 1000), now);
        scheduler.arrive(message("n1", Importance::Normal, 1000), now);
        scheduler.arrive(message("n2", Importance::Normal, 1000), now);
        assert_eq!(ids(&scheduler), vec!["n1", "h", "n2"]);
        assert_eq!(widths(&scheduler), vec![29.0, 42.0, 29.0]);
    }

    #[test]
    fn three_levels_center_high() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("l", Importance::Low, 1000), now);
        scheduler.arrive(message("n", Importance::Normal, 1000), now);
        scheduler.arrive(message("h", Importance::High, 1000), now);
        assert_eq!(ids(&scheduler), vec!["n", "h", "l"]);
        assert_eq!(widths(&scheduler), vec![30.0, 50.0, 20.0]);
    }

    #[test]
    fn two_high_one_normal_not_centered() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("h1", Importance::High, 1000), now);
        scheduler.arrive(message("h2", Importance::High, 1000), now);
        scheduler.arrive(message("n", Importance::Normal, 1000), now);
        assert_eq!(ids(&scheduler), vec!["h1", "h2", "n"]);
        assert_eq!(widths(&scheduler), vec![38.0, 38.0, 24.0]);
    }

    #[test]
    fn one_normal_two_low_centers_normal() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("n", Importance::Normal, 1000), now);
        scheduler.arrive(message("l1", Importance::Low, 1000), now);
        scheduler.arrive(message("l2", Importance::Low, 1000), now);
        assert_eq!(ids(&scheduler), vec!["l1", "n", "l2"]);
        assert_eq!(widths(&scheduler), vec![25.0, 50.0, 25.0]);
    }

    #[test]
    fn equal_importance_needs_no_centering() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        for id in ["a", "b", "c"] {
            scheduler.arrive(message(id, Importance::Normal, 1000), now);
        }
        assert_eq!(widths(&scheduler), vec![33.3, 33.3, 33.4]);
    }

    #[test]
    fn strictly_higher_preempts_oldest_of_lowest() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("l1", Importance::Low, 10000), now);
        scheduler.arrive(message("n1", Importance::Normal, 10000), now);
        scheduler.arrive(message("l2", Importance::Low, 10000), now);
        assert!(scheduler.arrive(message("h", Importance::High, 10000), now));
        assert!(!ids(&scheduler).contains(&"l1".to_string()));
        assert!(ids(&scheduler).contains(&"h".to_string()));
        assert_eq!(scheduler.snapshot().queue_count, 0);
    }

    #[test]
    fn same_importance_queues_instead_of_preempting() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        for id in ["a", "b", "c"] {
            scheduler.arrive(message(id, Importance::Normal, 10000), now);
        }
        scheduler.arrive(message("d", Importance::Normal, 10000), now);
        assert_eq!(ids(&scheduler), vec!["a", "b", "c"]);
        assert_eq!(scheduler.snapshot().queue_count, 1);
    }

    #[test]
    fn queue_orders_by_importance_then_arrival() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        for id in ["a", "b", "c"] {
            scheduler.arrive(message(id, Importance::High, 10000), now);
        }
        scheduler.arrive(message("low", Importance::Low, 10000), now);
        scheduler.arrive(message("normal", Importance::Normal, 10000), now);
        assert_eq!(scheduler.snapshot().queue_head, Some("normal".to_string()));
    }

    #[test]
    fn full_queue_kills_oldest_on_screen() {
        let now = base();
        let mut scheduler = Scheduler::new(60000);
        for id in ["h1", "h2", "h3"] {
            scheduler.arrive(message(id, Importance::High, 60000), now);
        }
        for index in 0..QUEUE_LIMIT {
            scheduler.arrive(
                message(&format!("q{index}"), Importance::Normal, 60000),
                now,
            );
        }
        assert_eq!(scheduler.snapshot().queue_count, QUEUE_LIMIT);
        let oldest = ids(&scheduler)[0].clone();
        assert_eq!(oldest, "h1");
        // 队列已满，再来一条普通消息：销毁屏上最早入场的 h1，再从队首补位。
        scheduler.arrive(message("extra", Importance::Normal, 60000), now);
        let after = ids(&scheduler);
        assert!(!after.contains(&"h1".to_string()), "最早的在屏消息应被销毁");
        assert_eq!(after.len(), SLOT_LIMIT);
        assert_eq!(scheduler.snapshot().queue_count, QUEUE_LIMIT);
    }

    #[test]
    fn tick_removes_expired_and_refills() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("a", Importance::Normal, 1000), now);
        scheduler.arrive(message("b", Importance::Low, 5000), now);
        assert!(!scheduler.tick(at(now, 500)));
        assert!(scheduler.tick(at(now, 1200)));
        assert_eq!(ids(&scheduler), vec!["b"]);
    }

    #[test]
    fn dedupe_arrival_updates_in_place_and_extends_deadline() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        scheduler.arrive(message("a", Importance::Normal, 10000), now);
        let mut again = message("a", Importance::Normal, 10000);
        again.count = 2;
        assert!(scheduler.arrive(again, at(now, 5000)));
        assert_eq!(ids(&scheduler), vec!["a"]);
        assert_eq!(scheduler.snapshot().slots[0].message.count, 2);
        // 续期到第二个到达时刻 + 10s，所以 12s 时还活着。
        assert!(!scheduler.tick(at(now, 12000)));
        assert!(scheduler.tick(at(now, 16000)));
    }

    #[test]
    fn dismiss_from_queue_leaves_slots_alone() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        for id in ["a", "b", "c"] {
            scheduler.arrive(message(id, Importance::Normal, 10000), now);
        }
        scheduler.arrive(message("queued", Importance::Low, 10000), now);
        assert!(scheduler.dismiss("queued", now));
        assert_eq!(ids(&scheduler), vec!["a", "b", "c"]);
        assert_eq!(scheduler.snapshot().queue_count, 0);
    }

    #[test]
    fn dismissing_a_slot_refills_from_queue() {
        let now = base();
        let mut scheduler = Scheduler::new(5000);
        for id in ["a", "b", "c"] {
            scheduler.arrive(message(id, Importance::Normal, 10000), now);
        }
        scheduler.arrive(message("queued", Importance::Low, 10000), now);
        assert!(scheduler.dismiss("b", now));
        let after = ids(&scheduler);
        assert_eq!(after.len(), 3);
        assert!(after.contains(&"queued".to_string()));
    }

    #[test]
    fn widths_always_add_up_to_hundred() {
        let now = base();
        for importance in [Importance::High, Importance::Normal, Importance::Low] {
            let mut scheduler = Scheduler::new(5000);
            for index in 0..SLOT_LIMIT {
                scheduler.arrive(
                    message(&format!("m{index}"), importance, 10000),
                    now,
                );
            }
            let total: f64 = widths(&scheduler).iter().sum();
            assert!((total - 100.0).abs() < 0.001, "宽度和应为 100，实际 {total}");
        }
    }
}
