use core::{
    cell::{Cell, UnsafeCell},
    mem::MaybeUninit,
    sync::atomic::{self, AtomicBool, AtomicU32, Ordering},
};

use bw_r_drivers_tc37x::uart::print;

use crate::{
    executor::{self, executor_impl::Executor, yields::Yielder},
    memory::alloc::{Alloc, ALLOC},
    mutex::{self, blocking::CriticalSectionRawMutex},
};

use super::Core;

/// signals between tasks and tasks, ISRs and task ON THE SAME CORE
static CORE0_LOCAL_EVENT: mutex::blocking::Mutex<CriticalSectionRawMutex, Cell<bool>> =
    mutex::blocking::Mutex::new(Cell::new(true));

// TODO: signals across cores, use CAS if possible??

// Core0 system ticks
static TICKS: AtomicU32 = AtomicU32::new(0);

/// memory size for the bump allocator used to allocate Futures
const MEMORY_SIZE: usize = 1024;

pub struct Core0 {}

impl Core for Core0 {
    const CORE_NR: u8 = 0;

    fn signal_event_local() {
        // CORE0_HAS_EVENT.store(true, Ordering::SeqCst);
        CORE0_LOCAL_EVENT.lock(|val| val.set(true));
    }

    fn event_fetch_and_clear_local() -> bool {
        // use LOAD_MODIFY_STORE instead
        // let has_event = CORE0_HAS_EVENT.load(Ordering::SeqCst);
        // CORE0_HAS_EVENT.store(false, Ordering::SeqCst);
        // has_event

        CORE0_LOCAL_EVENT.lock(|val| {
            let old = val.get();
            val.set(false);
            old
        })
    }

    fn is_current_core() -> bool {
        unimplemented!();
    }

    fn on_timer_interrupt() {
        print("Timer interrupt on core 0");
        TICKS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    }

    fn executor_ref() -> &'static Executor<Self> {
        static INIT: AtomicBool = AtomicBool::new(false); // initialised or not
        static mut EXECUTOR: UnsafeCell<MaybeUninit<Executor<Core0>>> =
            UnsafeCell::new(MaybeUninit::uninit());

        if INIT.load(Ordering::Relaxed) {
            unsafe { &*(EXECUTOR.get() as *const Executor<Self>) }
        } else {
            unsafe {
                // reserved memory for the bump allocator
                static mut MEMORY: [u8; MEMORY_SIZE] = [0; MEMORY_SIZE];

                let executorp = EXECUTOR.get() as *mut Executor<Self>;
                executorp.write(Executor::new());
                let allocp = ALLOC.get() as *mut Alloc;
                allocp.write(Alloc::new(&mut MEMORY));
                // force the `allocp` write to complete before returning from this function
                atomic::compiler_fence(Ordering::Release);
                INIT.store(true, Ordering::Relaxed);
                &*executorp
            }
        }
    }

    fn spawn(fut: impl core::future::Future<Output = ()> + 'static) {
        Self::executor_ref().spawn(fut);
    }

    fn start_executor() -> ! {
        Self::executor_ref().start();
    }

    fn yields() -> executor::yields::Yielder<Self>
    where
        Self: Sized,
    {
        Yielder::new()
    }
}
