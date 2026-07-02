// src-tauri/src/exotic/limiter.rs
//! 后台重活公平并发池（v3.1 勘误 R4）。
//!
//! **derivation 重型任务与 exotic Worker 请求共享同一个全局 permit 预算**——二者同级（总纲优先级阶梯），
//! 谁也不硬让步谁。若各开各的 semaphore，大视频库持续派生会饿死 exotic（PSD 永不出图）。
//!
//! 关键性质：
//!   - **FIFO 公平**：按到达顺序授予 permit（票号 + 队首服务），exotic 票一旦入队，下一个释放的
//!     permit 必落到它头上、不被后到的 derivation 票插队 → 最大排队等待有上界（R4 公平验收）。
//!   - **取消感知**：`acquire` 周期性醒来检查 `CancellationToken`；取消即退队返回 None，不泄漏票。
//!   - **即时释放**：`HeavyPermit` Drop 归还 permit 并唤醒队首（故障测试查无泄漏）。
//!
//! 调用约定（两条流水线一致）：在**派发线程**（非 rayon worker）取 permit → 移入任务闭包 → 任务
//! 完成/取消/kill 时 Drop 释放。派发线程阻塞在 acquire 即天然「预取不超过可派发容量」（R4 规则 4）。

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

/// 取消轮询周期：acquire 等待时每隔此时长醒来检查取消位（兼顾响应性与空转）。
const CANCEL_POLL: Duration = Duration::from_millis(100);

struct LimiterState {
    /// 当前可用 permit 数。
    available: usize,
    /// 下一张票号（单调递增）。
    next_ticket: u64,
    /// 等待队列（票号 FIFO）。队首才有资格在 available>0 时取走 permit。
    queue: VecDeque<u64>,
}

/// 公平后台重活池。`Arc` 共享给 derivation 与 exotic 两条流水线。
pub struct BackgroundHeavyLimiter {
    state: Mutex<LimiterState>,
    cv: Condvar,
    total: usize,
}

impl BackgroundHeavyLimiter {
    /// 以 `permits` 个并发额度创建（建议 = 后台重活目标并发，如 `available_parallelism()`）。
    pub fn new(permits: usize) -> Arc<Self> {
        let permits = permits.max(1);
        Arc::new(BackgroundHeavyLimiter {
            state: Mutex::new(LimiterState {
                available: permits,
                next_ticket: 0,
                queue: VecDeque::new(),
            }),
            cv: Condvar::new(),
            total: permits,
        })
    }

    /// 总额度。
    pub fn total(&self) -> usize {
        self.total
    }

    /// 当前可用额度（瞬时快照；仅供观测/测试）。
    pub fn available(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .available
    }

    /// 公平取一个 permit；阻塞直到拿到或 `token` 取消。取消返回 `None`（已退队，不泄漏）。
    pub fn acquire(self: &Arc<Self>, token: &CancellationToken) -> Option<HeavyPermit> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        state.queue.push_back(ticket);

        loop {
            if token.is_cancelled() {
                remove_ticket(&mut state.queue, ticket);
                // 退队可能让出队首 → 唤醒其余等待者重新评估。
                self.cv.notify_all();
                return None;
            }
            // 仅队首在有额度时取走（FIFO，杜绝插队 → 等待上界）。
            if state.available > 0 && state.queue.front() == Some(&ticket) {
                state.available -= 1;
                state.queue.pop_front();
                return Some(HeavyPermit {
                    limiter: Arc::clone(self),
                });
            }
            let (s, _timeout) = self
                .cv
                .wait_timeout(state, CANCEL_POLL)
                .unwrap_or_else(|e| e.into_inner());
            state = s;
        }
    }

    /// 释放一个 permit（仅由 [`HeavyPermit::drop`] 调用）。
    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.available += 1;
        // 唤醒全部等待者：只有当前队首会真正取走，其余继续等（Condvar 无法精确点名队首）。
        drop(state);
        self.cv.notify_all();
    }
}

/// 从队列中移除某票号（取消退队用）。
fn remove_ticket(queue: &mut VecDeque<u64>, ticket: u64) {
    if let Some(pos) = queue.iter().position(|&t| t == ticket) {
        queue.remove(pos);
    }
}

/// permit 持有凭证。Drop 即归还额度并唤醒队首。
pub struct HeavyPermit {
    limiter: Arc<BackgroundHeavyLimiter>,
}

impl Drop for HeavyPermit {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn caps_concurrency_and_releases() {
        let lim = BackgroundHeavyLimiter::new(2);
        let token = CancellationToken::new();
        let live = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let lim = Arc::clone(&lim);
            let token = token.clone();
            let live = Arc::clone(&live);
            let max_seen = Arc::clone(&max_seen);
            handles.push(thread::spawn(move || {
                let _permit = lim.acquire(&token).unwrap();
                let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(20));
                live.fetch_sub(1, Ordering::SeqCst);
                // permit drop 释放
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 任一时刻并发不超过额度 2。
        assert!(max_seen.load(Ordering::SeqCst) <= 2);
        // 全部归还。
        assert_eq!(lim.available(), 2);
    }

    #[test]
    fn cancel_unblocks_waiter_without_leak() {
        let lim = BackgroundHeavyLimiter::new(1);
        let token = CancellationToken::new();
        // 主线程占满唯一 permit。
        let held = lim.acquire(&token).unwrap();

        let lim2 = Arc::clone(&lim);
        let token2 = token.clone();
        let waiter = thread::spawn(move || lim2.acquire(&token2).is_none());

        // 给 waiter 时间进入等待，然后取消。
        thread::sleep(Duration::from_millis(50));
        token.cancel();
        assert!(waiter.join().unwrap(), "取消应使等待者返回 None");

        // 持有者释放后额度回到 1，无票泄漏（队列已清）。
        drop(held);
        assert_eq!(lim.available(), 1);
    }

    #[test]
    fn fifo_fairness_first_waiter_served_first() {
        // 额度 1：占满后 A 先入队、B 后入队；释放一次必先给 A。
        let lim = BackgroundHeavyLimiter::new(1);
        let token = CancellationToken::new();
        let held = lim.acquire(&token).unwrap();

        let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let mk = |name: &'static str, delay_ms: u64| {
            let lim = Arc::clone(&lim);
            let token = token.clone();
            let order = Arc::clone(&order);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(delay_ms));
                let _p = lim.acquire(&token).unwrap();
                order.lock().unwrap().push(name);
                thread::sleep(Duration::from_millis(10));
            })
        };
        let a = mk("A", 0);
        thread::sleep(Duration::from_millis(30)); // 确保 A 先入队
        let b = mk("B", 0);
        thread::sleep(Duration::from_millis(30));
        drop(held); // 释放 → 应先给 A

        a.join().unwrap();
        b.join().unwrap();
        let got = order.lock().unwrap().clone();
        assert_eq!(got, vec!["A", "B"], "FIFO：先入队者先获 permit");
    }
}
