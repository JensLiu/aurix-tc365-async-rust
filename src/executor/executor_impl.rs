use core::{
    cell::UnsafeCell, future::Future, marker::PhantomData, pin::Pin, sync::atomic::{AtomicBool, Ordering}, task::{Context, RawWaker, RawWakerVTable, Waker}
};

use bw_r_drivers_tc37x::uart::print;
use heapless::Vec;

use crate::{cpu::Core, memory::alloc::{Alloc, ALLOC}};

// waker context, p is a raw pointer to an atomic bool variable
static VTABLE: RawWakerVTable = {
    unsafe fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }

    unsafe fn wake(p: *const ()) {
        wake_by_ref(p);
    }

    unsafe fn wake_by_ref(p: *const ()) {
        (*(p as *const AtomicBool)).store(true, Ordering::Release)
    }

    unsafe fn drop(_: *const ()) {
        // no-op
    }

    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

const MAX_TASKS: usize = 32;

type Task = Node<dyn Future<Output = ()> + 'static>;

pub struct Node<F>
where
    F: ?Sized,
{
    ready: AtomicBool,
    f: UnsafeCell<F>,
}
// UnsafeCell for interior mutability while still allowing it to be static

impl Task {
    pub fn new(fut: impl Future + 'static) -> &'static mut Self {
        unsafe {
            // ALLOC should have already been initialised at this point
            let alloc = ALLOC.get() as *mut Alloc;
            (*alloc).alloc_init(Node {
                ready: AtomicBool::new(true),
                f: UnsafeCell::new(async {
                    fut.await;
                    // `spawn`-ed tasks must never terminate
                    crate::utils::abort();
                }),
            })
        }
    }
}

pub struct Executor<A> {
    // not using mutex for now
    // we are only testing on a single core and it will
    // only be mutated in thread mode, hence no preemption
    tasks: UnsafeCell<Vec<&'static Task, MAX_TASKS>>,
    _phantom: PhantomData<A>,   // A for arch
}

impl<A: Core> Executor<A> {
    pub fn new() -> Self {
        Self {
            tasks: UnsafeCell::new(Vec::new()),
            _phantom: PhantomData,
        }
    }

    pub fn spawn(&self, f: impl Future + 'static) {
        let res = unsafe { (*self.tasks.get()).push(Task::new(f)) };
        if res.is_err() {
            // OOM
            crate::utils::abort();
        }
    }

    pub fn start(&self) -> ! {
        let mut some_task_advanced = false;
        // advance each runnable task
        let tasks = unsafe { &mut (*self.tasks.get()) };
        loop {
            
            if !A::event_fetch_and_clear_local() {
                // If events arrive here, it will not be lost
                // and will be noficed in the next iteration of this buzy loop.
                continue;
            }

            // print("[Debug]: executor woken up\n");
            for task in tasks.iter() {
                if task.ready.load(Ordering::SeqCst) {
                    some_task_advanced = true;

                    // set to suspended because it has been polled
                    task.ready.store(false, Ordering::Release);

                    let waker = unsafe {
                        Waker::from_raw(RawWaker::new(
                            &task.ready as *const _ as *const _, // as argument to the Waker::wake function
                            &VTABLE,
                        ))
                    };

                    let mut cx = Context::from_waker(&waker);

                    let _ = unsafe {
                        // SAFETY: It points to a static memory location, so it is naturally Pin-ed
                        let fut = Pin::new_unchecked(&mut *task.f.get());
                        fut.poll(&mut cx).is_ready()
                    };
                }
            }

            // if some_task_advanced {
            //     A::signal_event_local();
            // }
        }
    }
}
