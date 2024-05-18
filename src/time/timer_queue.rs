use core::{
    future::Future,
    sync::atomic::{AtomicU32, Ordering},
    task::{Poll, Waker},
};

use heapless::{binary_heap, BinaryHeap};

use crate::{
    executor::{
        task::{TaskHeader, TaskRef},
        waker,
    },
    print,
};

// interrupt handler for the priority 6 task on core 0 (vector table 0)

#[macro_export]
macro_rules! interrupt_handler_gen {
    ($int_nr:expr, $code:block) => {
        paste::psate! {
            #[no_mangle]
            extern "C" fn [<__INTERRUPT_HANDLER_$int_nr>]() {
                $code
            }
        }
    };
    ($int_nr:expr) => {
        paste::paste! {
            #[no_mangle]
            extern "C" fn [<__INTERRUPT_HANDLER_$int_nr>]() {
                print("interrupt being called");
                loop {}
            }
        }
    };
}

// pub struct TimerFut {
//     expires_at: ,
// }

// impl TimerFut {
//     pub fn has_expired(&self) -> bool {
//         TICKS.load(Ordering::SeqCst) >= self.at_ticks
//     }
// }

// impl Future for TimerFut {
//     type Output = ();

//     fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
//         if self.has_expired() {
//             cx.waker().wake_by_ref();
//             core_0_signal_event();
//             Poll::Ready(())
//         } else {
//             Poll::Pending
//         }
//     }
// }

// const fn gcd(a: u64, b: u64) -> u64 {
//     if b == 0 {
//         a
//     } else {
//         gcd(b, a % b)
//     }
// }

// pub const TICK_HZ: u64 = 20_000;

// pub(crate) const GCD_1K: u64 = gcd(TICK_HZ, 1_000);
// pub(crate) const GCD_1M: u64 = gcd(TICK_HZ, 1_000_000);
// pub(crate) const GCD_1G: u64 = gcd(TICK_HZ, 1_000_000_000);

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
pub struct TimerQueue {
    queue: BinaryHeap<TimerQueueItem, binary_heap::Min, N_TIMERS>,
}

impl TimerQueue {
    pub fn new() -> Self {
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
