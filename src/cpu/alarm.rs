use heapless::{binary_heap, BinaryHeap, Vec};

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
    callbacks: Vec<Option<(fn(*mut ()), *mut ())>, N_HANDLES>,
    queue: BinaryHeap<AlarmItem, binary_heap::Min, N_ALARMS>,
}

impl<const H: usize, const A: usize> AlarmQueue<H, A> {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
            queue: BinaryHeap::new(),
        }
    }

    pub fn try_allocate_handle(&mut self) -> Option<AlarmHandle> {
        self.callbacks.push(None).ok()?;
        let idx = self.callbacks.len() - 1;
        Some((idx as u8).into())
    }

    pub fn register_callback(&mut self, handle: AlarmHandle, f: fn(*mut ()), ctx: *mut ()) {
        let idx = handle.id as usize;
        let x = self.callbacks.get_mut(idx).unwrap();
        *x = Some((f, ctx));
    }

    fn call(&self, handle: AlarmHandle) -> Option<()> {
        let idx = handle.id;
        if let Some((f, cx)) = self.callbacks.get(idx as usize)? {
            f(cx.clone());
            return Some(());
        }
        None
    }

    pub fn set_alarm(&mut self, handle: AlarmHandle, expires_at: u64) -> Option<()> {
        self.queue.push(AlarmItem { expires_at, handle }).ok()
    }

    pub fn call_all_expired(&mut self, now: u64) {
        // this is a min heap
        while let Some(item) = self.queue.peek() {
            if item.expires_at <= now {
                let handle = self.queue.pop().unwrap().handle;
                self.call(handle);
            } else {
                break;
            }
        }
    }

    pub fn queue_size(&self) -> usize {
        self.queue.len()
    }
}
