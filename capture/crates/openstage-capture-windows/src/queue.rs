// SPDX-License-Identifier: Apache-2.0
//! A short waiting line for screen pictures that always keeps the newest ones.
//!
//! Windows hands over pictures as fast as the screen changes, which can be faster than they are
//! used. When the line is full the OLDEST picture is thrown away, not the newest, because a video
//! at a steady rate wants the latest picture of the screen, never a stale one.

use crate::Frame;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

struct State {
    frames: VecDeque<Frame>,
    replaced: u64,
}

/// A waiting line shared between the thread Windows delivers pictures on and the thread that uses them.
pub struct FrameQueue {
    capacity: usize,
    state: Mutex<State>,
    arrived: Condvar,
}

impl FrameQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(State { frames: VecDeque::new(), replaced: 0 }),
            arrived: Condvar::new(),
        }
    }

    /// Add a picture. If the line is full, the oldest picture is dropped to make room.
    pub fn push(&self, frame: Frame) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.frames.len() >= self.capacity {
            state.frames.pop_front();
            state.replaced += 1;
        }
        state.frames.push_back(frame);
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
