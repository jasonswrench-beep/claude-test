// SPDX-License-Identifier: Apache-2.0
//! A short waiting line for screen pictures that always keeps the newest ones.
//!
//! Windows hands over pictures as fast as the screen changes, which can be faster than they are
//! used. When the line is full the OLDEST picture is thrown away, not the newest, because a video
//! at a steady rate wants the latest picture of the screen, never a stale one.

use crate::Frame;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Memory for pictures, kept for reuse.
///
/// A 1080p picture is about 8 megabytes. Allocating that fresh for every picture and handing it back to
/// Windows afterwards makes the system find, clear and map two thousand pages each time, dozens of times a
/// second, which is processor time spent on nothing. A few buffers kept here are used over and over.
pub struct FramePool {
    free: Mutex<Vec<Vec<u8>>>,
    reused: AtomicU64,
}

impl FramePool {
    /// The most buffers kept; a picture handed back beyond that is let go.
    pub const KEEP: usize = 6;

    pub fn new() -> Self {
        Self { free: Mutex::new(Vec::new()), reused: AtomicU64::new(0) }
    }

    /// A buffer of exactly `len` bytes: a kept one if its size is right, else a new zeroed one.
    /// A kept buffer still holds an earlier picture; whoever takes it must overwrite all of it.
    pub fn take(&self, len: usize) -> Vec<u8> {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        // A change of size (the display was changed) makes the kept buffers useless: let them go.
        if free.iter().any(|b| b.len() != len) {
            free.retain(|b| b.len() == len);
        }
        match free.pop() {
            Some(buffer) => {
                self.reused.fetch_add(1, Ordering::Relaxed);
                buffer
            }
            None => vec![0u8; len],
        }
    }

    /// Keep a buffer for later, unless enough are kept already.
    pub fn give(&self, buffer: Vec<u8>) {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        if free.len() < Self::KEEP {
            free.push(buffer);
        }
    }

    /// How many times a kept buffer was used instead of new memory.
    pub fn reused(&self) -> u64 {
        self.reused.load(Ordering::Relaxed)
    }
}

impl Default for FramePool {
    fn default() -> Self {
        Self::new()
    }
}

struct State {
    frames: VecDeque<Frame>,
    replaced: u64,
    /// How many pictures were ever added.
    pushed: u64,
    /// The most pictures that were ever waiting at once.
    most_waiting: usize,
}

/// A waiting line shared between the thread Windows delivers pictures on and the thread that uses them.
pub struct FrameQueue {
    capacity: usize,
    state: Mutex<State>,
    arrived: Condvar,
    pool: FramePool,
}

impl FrameQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(State { frames: VecDeque::new(), replaced: 0, pushed: 0, most_waiting: 0 }),
            arrived: Condvar::new(),
            pool: FramePool::new(),
        }
    }

    /// Memory for a picture of `len` bytes, reused when possible (see [`FramePool`]).
    pub fn take_buffer(&self, len: usize) -> Vec<u8> {
        self.pool.take(len)
    }

    /// A picture nobody needs any more: keep its memory for the next one.
    pub fn recycle(&self, frame: Frame) {
        self.pool.give(frame.bgra);
    }

    /// How many pictures were put into memory that an earlier picture had used.
    pub fn reused(&self) -> u64 {
        self.pool.reused()
    }

    /// Add a picture. If the line is full, the oldest picture is dropped to make room.
    pub fn push(&self, frame: Frame) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.frames.len() >= self.capacity {
            if let Some(oldest) = state.frames.pop_front() {
                self.pool.give(oldest.bgra);
            }
            state.replaced += 1;
        }
        state.frames.push_back(frame);
        state.pushed += 1;
        state.most_waiting = state.most_waiting.max(state.frames.len());
        drop(state);
        self.arrived.notify_one();
    }

    /// The oldest waiting picture, waiting up to `timeout` for one to arrive.
    pub fn pop(&self, timeout: Duration) -> Option<Frame> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return Some(frame);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (next, _) = self.arrived.wait_timeout(state, deadline - now).unwrap_or_else(|e| e.into_inner());
            state = next;
        }
    }

    /// How many pictures were dropped to make room for newer ones.
    pub fn replaced(&self) -> u64 {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).replaced
    }

    /// How many pictures were ever added.
    pub fn pushed(&self) -> u64 {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).pushed
    }

    /// The most pictures that were ever waiting at once.
    pub fn most_waiting(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).most_waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn pic(t: u64) -> Frame {
        Frame { width: 1, height: 1, t_ms: t, captured_100ns: None, arrived_100ns: None, bgra: vec![0; 4] }
    }

    #[test]
    fn pictures_come_out_oldest_first() {
        let q = FrameQueue::new(4);
        q.push(pic(1));
        q.push(pic(2));
        assert_eq!(q.pop(Duration::ZERO).unwrap().t_ms, 1);
        assert_eq!(q.pop(Duration::ZERO).unwrap().t_ms, 2);
        assert!(q.pop(Duration::ZERO).is_none());
        assert_eq!(q.replaced(), 0);
    }

    #[test]
    fn a_full_line_drops_the_oldest_and_keeps_the_newest() {
        let q = FrameQueue::new(3);
        for t in 1..=5 {
            q.push(pic(t));
        }
        assert_eq!(q.replaced(), 2);
        let kept: Vec<u64> = std::iter::from_fn(|| q.pop(Duration::ZERO)).map(|f| f.t_ms).collect();
        assert_eq!(kept, vec![3, 4, 5]);
    }

    #[test]
    fn the_queue_counts_what_came_in_and_the_most_that_ever_waited() {
        let q = FrameQueue::new(4);
        for t in 1..=3 {
            q.push(pic(t));
        }
        q.pop(Duration::ZERO);
        q.pop(Duration::ZERO);
        for t in 4..=9 {
            q.push(pic(t));
        }
        assert_eq!(q.pushed(), 9);
        assert_eq!(q.most_waiting(), 4, "never more than the line holds");
        assert_eq!(q.replaced(), 3);
    }

    #[test]
    fn memory_handed_back_is_used_again_instead_of_asking_for_new() {
        let q = FrameQueue::new(4);
        let first = q.take_buffer(1000);
        let address = first.as_ptr();
        assert_eq!(q.reused(), 0, "the first buffer is new");
        q.recycle(Frame { bgra: first, ..pic(1) });
        let second = q.take_buffer(1000);
        assert_eq!(second.as_ptr(), address, "the same memory came back");
        assert_eq!(q.reused(), 1);
    }

    #[test]
    fn a_picture_thrown_away_for_a_newer_one_gives_its_memory_to_the_next() {
        let q = FrameQueue::new(2);
        for t in 1..=3 {
            q.push(Frame { bgra: q.take_buffer(64), ..pic(t) });
        }
        assert_eq!(q.replaced(), 1);
        let _ = q.take_buffer(64);
        assert_eq!(q.reused(), 1, "the dropped picture's memory was kept, not freed");
    }

    #[test]
    fn memory_of_the_wrong_size_is_not_reused_and_is_let_go() {
        let pool = FramePool::new();
        pool.give(vec![7u8; 100]);
        let other = pool.take(200);
        assert_eq!((other.len(), pool.reused()), (200, 0), "a smaller buffer cannot hold a bigger picture");
        assert!(other.iter().all(|&b| b == 0), "new memory is cleared");
        // The old, wrong-sized buffer is gone, not kept for ever.
        assert_eq!(pool.take(100).len(), 100);
        assert_eq!(pool.reused(), 0);
    }

    #[test]
    fn only_a_few_buffers_are_kept() {
        let pool = FramePool::new();
        for _ in 0..FramePool::KEEP + 10 {
            pool.give(vec![0u8; 16]);
        }
        for _ in 0..FramePool::KEEP {
            let _ = pool.take(16);
        }
        assert_eq!(pool.reused(), FramePool::KEEP as u64);
        let _ = pool.take(16);
        assert_eq!(pool.reused(), FramePool::KEEP as u64, "the rest were let go");
    }

    #[test]
    fn waiting_gives_up_after_the_timeout() {
        let q = FrameQueue::new(2);
        let started = Instant::now();
        assert!(q.pop(Duration::from_millis(60)).is_none());
        assert!(started.elapsed() >= Duration::from_millis(55));
    }

    #[test]
    fn a_waiting_reader_is_woken_when_a_picture_arrives() {
        let q = Arc::new(FrameQueue::new(2));
        let writer = Arc::clone(&q);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            writer.push(pic(9));
        });
        let started = Instant::now();
        let got = q.pop(Duration::from_secs(5)).expect("woken by the push");
        assert_eq!(got.t_ms, 9);
        assert!(started.elapsed() < Duration::from_secs(2), "it did not wait for the whole timeout");
        handle.join().unwrap();
    }
}
