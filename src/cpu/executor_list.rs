use core::sync::atomic::{AtomicBool, Ordering};

use bw_r_drivers_tc37x::interrupt::SoftwareInterruptNode;
use heapless::LinearMap;

use crate::{
    executor::{interrupt_executor::InterruptExecutor, thread_executor::ThreadExecutor}, install_interrupt_executor_handler, print, sync::blocking_mutex::{Mutex, RawMutex}
};

use super::{cpu0::Kernel, Cpu};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ExecutorHandle {
    prio: u8,
}

impl ExecutorHandle {
    pub unsafe fn from_prio(prio: u8) -> Self {
        Self { prio }
    }

    pub fn get_prio(&self) -> u8 {
        self.prio
    }
}

#[derive(Debug)]
pub struct ExecutorList<R, const N: usize> {
    // this mutex just makes it sync
    thread_executor: Mutex<R, ThreadExecutor>,
    // this mutex protects the structure of the `executors` map
    executors: Mutex<R, LinearMap<ExecutorHandle, InterruptExecutor, N>>,
}

impl<R: RawMutex, const N: usize> ExecutorList<R, N> {
    pub const fn new() -> Self {
        Self {
            thread_executor: Mutex::new(ThreadExecutor::new(None)),
            executors: Mutex::new(LinearMap::new()),
        }
    }

    pub fn thread_executor(&self) -> &'static ThreadExecutor {
        static INITIALISED: AtomicBool = AtomicBool::new(false);
        if !INITIALISED.load(Ordering::SeqCst) {
            let alarm = Kernel::allocate_alarm().unwrap();
            self.thread_executor.lock(|x| x.register_alarm(alarm));
            INITIALISED.store(true, Ordering::SeqCst);
        }
        // cannot return an executor with critical section lock because it will
        // disable interrupts hence priority-based preemption
        let x = self.thread_executor.lock(|x| x as *const ThreadExecutor);
        unsafe {
            // SAFETY: it only gives out immutable reference of the thread executor
            &*x
        }
    }

    pub fn allocate(&self, node: SoftwareInterruptNode) -> Option<ExecutorHandle> {
        if node.get_prio() == 0 {
            panic!("ExecutorList::allocate zero priority node {:#?}\n", node);
        }

        let handle = ExecutorHandle {
            prio: node.get_prio(),
        };

        // try allocating alarm handle
        let alarm = match Kernel::allocate_alarm() {
            Some(x) => x,
            None => panic!("ExecutorList::allocate: unable to allocate alarm"),
        };

        // try allocating executors
        match self
            .executors
            .lock_mut(|x| x.insert(handle, InterruptExecutor::new(node, Some(alarm))))
        {
            Ok(_) => {
                // enable interrupts
                node.init();
                node.enable();
                Some(handle)
            }
            // TODO: deallocate alarm handle on error?
            Err(_) => panic!("ExecutorList::allocate: unable to allocate executor"),
        }
    }

    pub fn spawn(
        &self,
        handle: ExecutorHandle,
        fut: impl core::future::Future<Output = ()> + 'static,
        name: &'static str,
    ) {
        self.executors.lock_mut(|x| {
            match x.get_mut(&handle) {
                None => panic!("spawn_executor: executor not allocated"),
                Some(e) => {
                    // hacked: make lifetime static
                    let e = unsafe { &mut *(e as *mut InterruptExecutor) };
                    e.spawn(fut, name)
                }
            }
        })
    }

    pub fn get(&self, handle: ExecutorHandle) -> &'static InterruptExecutor {
        // we cannot return a Mutex with CriticalSectionRawMutex protected by disabling interrupts
        // because it needs to be preemptive, otherwise there's no use for the `InterruptExecutor`
        let x = self
            .executors
            .lock(|x| x.get(&handle).unwrap() as *const InterruptExecutor);

        // hacked: make static lifetime
        unsafe {
            // SAFETY: it only gives out the immutable reference of the interrupt executor?
            &*x
        }
    }

    pub fn start_interrupt_executor(&self, handle: ExecutorHandle) {
        // print!("Start executor {:#?}\n", handle);
        self.executors.lock(|x| {
            let e = x.get(&handle).unwrap();
            e.start();
        })
    }
}
