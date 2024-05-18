use core::{future::Future, ptr::NonNull};

use crate::memory::alloc::{Alloc, ALLOC};

use super::executor_impl::Executor;

#[derive(Debug)]
pub struct TaskRef {
    pub(crate) header: NonNull<TaskHeader>,
}

impl TaskRef {
    pub unsafe fn from_ptr(ptr: *const TaskHeader) -> Self {
        Self {
            header: NonNull::new_unchecked(ptr as *mut TaskHeader),
        }
    }

    pub unsafe fn as_static_mut_header(&self) -> &'static mut TaskHeader {
        unsafe { &mut *self.header.as_ptr() }
    }
}

impl From<&'static mut TaskHeader> for TaskRef {
    fn from(value: &'static mut TaskHeader) -> Self {
        unsafe { Self::from_ptr(value as *const TaskHeader) }
    }
}

pub struct TaskHeader {
    pub(crate) name: &'static str,
    pub(crate) fut_ref: &'static mut dyn Future<Output = ()>,
    pub(crate) executor: &'static Executor,
    pub(crate) expires_at: Option<u64>,
}

impl TaskHeader {
    pub fn new(fut: impl Future + 'static, executor: &'static Executor, name: &'static str) -> &'static mut Self {
        let fut_ref = Self::allocate_static_future(fut);
        Self::use_alloc(|| Self {
            name,
            fut_ref,
            executor,
            expires_at: None,
        })
    }

    fn allocate_static_future(fut: impl Future + 'static) -> &'static mut dyn Future<Output = ()> {
        Self::use_alloc(|| {
            async {
                fut.await;
                panic!("Spawned task exited\n"); // `spawn`-ed tasks must never terminate
            }
        })
    }

    fn use_alloc<T>(f: impl FnOnce() -> T) -> &'static mut T {
        unsafe {
            // ALLOC should have already been initialised at this point
            let alloc = ALLOC.get() as *mut Alloc;
            (*alloc).alloc_init(f())
        }
    }
}
