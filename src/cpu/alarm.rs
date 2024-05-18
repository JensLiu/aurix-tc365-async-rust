use heapless::{binary_heap, BinaryHeap, Vec};

use crate::sync::blocking_mutex::{CriticalSectionRawMutex, Mutex};

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct AlarmHandle {
    id: u8,
}

impl From<u8> for AlarmHandle {
    fn from(value: u8) -> Self {
        Self { id: value }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AlarmItem {
    expires_at: u64, // compared first
    handle: AlarmHandle,
}

pub struct AlarmQueue<const N_HANDLES: usize, const N_ALARMS: usize> {
    // index as alarm handle
    callbacks: Mutex<CriticalSectionRawMutex, Vec<Option<(fn(*mut ()), *mut ())>, N_HANDLES>>,
    queue: Mutex<CriticalSectionRawMutex, BinaryHeap<AlarmItem, binary_heap::Min, N_ALARMS>>,
}

impl<const H: usize, const A: usize> AlarmQueue<H, A> {
    pub fn new() -> Self {
        Self {
            callbacks: Mutex::new(Vec::new()),
            queue: Mutex::new(BinaryHeap::new()),
        }
    }

    pub fn try_allocate_handle(&mut self) -> Option<AlarmHandle> {
        self.callbacks.lock_mut(|x| {
            x.push(None).ok()?;
            let idx = x.len() - 1;
            Some((idx as u8).into())
        })
    }

    pub fn register_callback(&mut self, handle: AlarmHandle, f: fn(*mut ()), ctx: *mut ()) {
        let idx = handle.id as usize;
        let x = self.callbacks.lock_mut(|x| {
            let ptr = x.get_mut(idx).unwrap();
            *ptr = Some((f, ctx));
        });
    }

    fn call(&self, handle: AlarmHandle) -> Option<()> {
        let idx = handle.id;
        if let Some((f, cx)) = self
            .callbacks
            .lock(|x| x.get(idx as usize).unwrap().clone())
        {
            f(cx.clone());
            return Some(());
        }
        None
    }

    pub fn set_alarm(&mut self, handle: AlarmHandle, expires_at: u64) -> Option<()> {
        self.queue
            .lock_mut(|x| x.push(AlarmItem { expires_at, handle }))
            .ok()
    }

    fn get_one_expired_item(&self, now: u64) -> Option<AlarmItem> {
        self.queue.lock_mut(|x| {
            if let Some(item) = x.peek() {
                if item.expires_at <= now {
                    return x.pop();
                }
            }
            None
        })
    }
    
    pub fn call_all_expired(&self, now: u64) {
        // this is a min heap
        while let Some(item) = self.get_one_expired_item(now) {
            if item.expires_at <= now {
                self.call(item.handle);
            } else {
                break;
            }
        }
    }
}
