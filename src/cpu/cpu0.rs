use core::{
    arch::asm,
    cell::{Cell, UnsafeCell},
    mem::MaybeUninit,
    sync::atomic::{self, AtomicBool, AtomicU32, Ordering},
};

use crate::{
    executor::{self, executor_impl::Executor, yields::Yielder},
    memory::alloc::{Alloc, ALLOC},
    mutex::blocking::{CriticalSectionRawMutex, Mutex},
    print,
};

use super::{
    alarm::{AlarmHandle, AlarmQueue},
    Cpu,
};

/// signals between tasks and tasks, ISRs and task ON THE SAME CORE
static CORE0_LOCAL_EVENT: Mutex<CriticalSectionRawMutex, Cell<bool>> = Mutex::new(Cell::new(true));

// TODO: signals across cores, use CAS if possible??

// Core0 system ticks
static TICKS: AtomicU32 = AtomicU32::new(0);

/// memory size for the bump allocator used to allocate Futures
const MEMORY_SIZE: usize = 1024;

pub struct Cpu0 {}

impl Cpu for Cpu0 {
    const CPU_NR: u8 = 0;

    fn signal_event_local() {
        // CORE0_HAS_EVENT.store(true, Ordering::SeqCst);
        CORE0_LOCAL_EVENT.lock(|val| val.set(true));
    }

    fn event_fetch_and_clear_local() -> bool {
        CORE0_LOCAL_EVENT.lock(|val| {
            let old = val.get();
            val.set(false);
            old
        })
    }

    fn now() -> u64 {
        TICKS.load(Ordering::SeqCst) as u64
    }

    fn on_timer_interrupt() {
        let now = TICKS.fetch_add(1, core::sync::atomic::Ordering::SeqCst) as u64;
        let r = unsafe {
            // SAFETY: a core can only call its own `Core` object
            // and interrupts are disabled, so no preemption
            &mut *ALARM_QUEUE.as_mut_ptr()
        };
        r.call_all_expired(now);
    }

    fn spawn(fut: impl core::future::Future<Output = ()> + 'static, name: &'static str) {
        unsafe { Self::executor_ref() }.spawn(fut, name);
    }

    fn start_executor() -> ! {
        unsafe { Self::executor_ref() }.start();
    }

    fn yields() -> executor::yields::Yielder<Self>
    where
        Self: Sized,
    {
        Yielder::new()
    }

    fn allocate_alarm() -> Option<AlarmHandle> {
        Self::modify_alarm_queue(|x| x.try_allocate_handle())
    }

    fn set_alarm_callback(handle: AlarmHandle, callback: fn(*mut ()), context: *mut ()) {
        // print!("[debug]: core0: set_alarm_callback for id: {:?}\n", handle);
        Self::modify_alarm_queue(|x| x.register_callback(handle, callback, context));
    }

    fn set_alarm(handle: AlarmHandle, expires_at: u64) -> Option<()> {
        // print!("[debug]: core0: {:?} set_alarm at {}\n", handle, expires_at);
        Self::modify_alarm_queue(|x| x.set_alarm(handle, expires_at))
    }

    fn init() {
        unsafe {
            let p = ALARM_QUEUE.as_mut_ptr();
            *p = AlarmQueue::new();
        }
        print!("core0 alarm queue init\n");

        bw_r_drivers_tc37x::timer::init_gpt12_timer();
        print!("core0 timer init\n");

        unsafe {
            asm!("enable");
        }
        print!("core0 enable interrupts");

        unsafe {
            INITIALISED.store(true, Ordering::SeqCst);
        }
    }

    unsafe fn executor_ref() -> &'static mut Executor {
        static INIT: AtomicBool = AtomicBool::new(false); // initialised or not
        static mut EXECUTOR: UnsafeCell<MaybeUninit<Executor>> =
            UnsafeCell::new(MaybeUninit::uninit());

        if INIT.load(Ordering::Relaxed) {
            &mut *(EXECUTOR.get() as *mut Executor)
        } else {
            // reserved memory for the bump allocator
            static mut MEMORY: [u8; MEMORY_SIZE] = [0; MEMORY_SIZE];

            let executorp = EXECUTOR.get() as *mut Executor;
            let alarm_handle = Self::allocate_alarm().unwrap(); // must succeed
            executorp.write(Executor::new(Some(alarm_handle)));
            let allocp = ALLOC.get() as *mut Alloc;
            allocp.write(Alloc::new(&mut MEMORY));
            // force the `allocp` write to complete before returning from this function
            atomic::compiler_fence(Ordering::Release);
            INIT.store(true, Ordering::Relaxed);
            &mut *executorp
        }
    }
}

impl Cpu0 {
    fn modify_alarm_queue<R>(
        f: impl FnOnce(&mut AlarmQueue<N_ALARM_HANDLES, N_ALARM_ITEMS>) -> R,
    ) -> R {
        let r = unsafe {
            // SAFETY: a core can only call its own `Core` object
            // and interrupts are disabled, so no preemption
            if !INITIALISED.load(Ordering::SeqCst) {
                panic!("ERROR: uninitialised alarm queue");
            }
            &mut *ALARM_QUEUE.as_mut_ptr()
        };
        f(r)
    }
}

// alarm queue
const N_ALARM_HANDLES: usize = 1;
const N_ALARM_ITEMS: usize = 64;

static mut ALARM_QUEUE: MaybeUninit<AlarmQueue<N_ALARM_HANDLES, N_ALARM_ITEMS>> =
    MaybeUninit::uninit();

// initialised flag
static mut INITIALISED: AtomicBool = AtomicBool::new(false);

// timer interrupt
#[no_mangle]
extern "C" fn __INTERRUPT_HANDLER_6() {
    Cpu0::on_timer_interrupt();
}
