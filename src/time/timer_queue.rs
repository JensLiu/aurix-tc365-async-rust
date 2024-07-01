use core::task::Waker;

use heapless::{binary_heap, BinaryHeap};

use crate::executor::{task::TaskRef, waker};

#[derive(Debug)]
pub struct TimerQueueItem {
    pub expires_at: u64,
    pub waker: Waker, // <- we stores the task ref in the waker
}

impl TimerQueueItem {
    pub fn get_task_ref(&self) -> TaskRef {
        waker::task_from_waker(&self.waker)
    }
}

impl PartialEq for TimerQueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.expires_at == other.expires_at
    }
}

impl Eq for TimerQueueItem {}

impl PartialOrd for TimerQueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.expires_at.partial_cmp(&other.expires_at)
    }
}

impl Ord for TimerQueueItem {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.expires_at.cmp(&other.expires_at)
    }
}

const N_TIMERS: usize = 64;
#[derive(Debug)]
pub struct TimerQueue {
    queue: BinaryHeap<TimerQueueItem, binary_heap::Min, N_TIMERS>,
}

impl TimerQueue {
    pub const fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }
    pub fn enqueue(&mut self, item: TimerQueueItem) -> Option<()> {
        self.queue.push(item).ok()
    }

    pub fn dequeue_expired(&mut self, now: u64, mut on_each: impl FnMut(TimerQueueItem)) -> usize {
        let mut x: usize = 0;
        loop {
            if let Some(item) = self.queue.peek() {
                // let task = unsafe { waker::task_from_waker(&item.waker).as_static_mut_header() };
                // print!(
                //     "\ttimer queue: task={}, expires_at={}, now={}\n",
                //     task.name, item.expires_at, now
                // );
                if item.expires_at <= now {
                    let item = self.queue.pop().unwrap();
                    on_each(item);
                    x += 1;
                } else {
                    // expires > now
                    break;
                }
            } else {
                break;
            }
        }
        x
    }

    pub fn next_expiration(&self) -> Option<u64> {
        if let Some(item) = self.queue.peek() {
            return Some(item.expires_at);
        }
        None
    }
}
