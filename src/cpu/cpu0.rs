use core::{
    arch::asm,
    borrow::Borrow,
    cell::{Cell, UnsafeCell},
    mem::MaybeUninit,
    sync::atomic::{self, AtomicBool, AtomicU32, Ordering},
};

use bw_r_drivers_tc37x::{gpio::GpioExt, interrupt::SoftwareInterruptNode, pac};
use embedded_alloc::Heap;
use heapless::{LinearMap, Vec};

use crate::{
    executor::{
        self, interrupt_executor::InterruptExecutor, thread_executor::ThreadExecutor,
        yields::Yielder,
    },
    memory::bump::{BumpAllocator, ALLOC},
    print,
    sync::blocking_mutex::{CriticalSectionRawMutex, Mutex},
};

use super::{
    alarm::{AlarmHandle, AlarmQueue},
    executor_list::{ExecutorHandle, ExecutorList},
    Cpu,
};

/// signals between tasks and tasks, ISRs and task ON THE SAME CORE
static CORE0_LOCAL_EVENT: Mutex<CriticalSectionRawMutex, Cell<bool>> = Mutex::new(Cell::new(true));

// TODO: signals across cores, use CAS if possible??

// Core0 system ticks
static TICKS: AtomicU32 = AtomicU32::new(0);

/// memory size for the bump allocator used to allocate Futures
const MEMORY_SIZE: usize = 1024 * 10;

// heap allocator
#[global_allocator]
static HEAP: embedded_alloc::Heap = embedded_alloc::Heap::empty();

pub struct Kernel {}

impl Cpu for Kernel {
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
        let n = r.call_all_expired(now);
    }

    fn spawn(fut: impl core::future::Future<Output = ()> + 'static, name: &'static str) {
        print!("KERNEL: Spawned {} at priority 0\n", name);
        unsafe { Self::thread_executor_ref() }.spawn(fut, name);
    }

    fn start_thread_executor() -> ! {
        print!("KERNEL: Start thread executor at prio 0\n");
        unsafe { Self::thread_executor_ref() }.start();
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
        // print!("[debug]: CORE0: set_alarm_callback for id: {:?}\n", handle);
        Self::modify_alarm_queue(|x| x.register_callback(handle, callback, context));
    }

    fn set_alarm(handle: AlarmHandle, expires_at: u64) -> Option<()> {
        // print!("[debug]: CORE0: {:?} set_alarm at {}\n", handle, expires_at);
        Self::modify_alarm_queue(|x| x.set_alarm(handle, expires_at))
    }

    fn init() {
        {
            unsafe {
                let p = ALARM_QUEUE.as_mut_ptr();
                *p = AlarmQueue::new();
            }
            print!("KERNEL: Alarm queue initialised\n");
        }

        // let list: ExecutorList<CriticalSectionRawMutex, N_EXECUTORS> = ExecutorList::new();
        // unsafe {
        //     print!("Map created\n");
        //     let p: *mut ExecutorList<CriticalSectionRawMutex, N_EXECUTORS> =
        //         INTERRUPT_EXECUTORS.as_mut_ptr();
        //     // *p = list;
        // }
        // print!("KERNEL: Executor list initialised\n");

        {
            bw_r_drivers_tc37x::timer::init_gpt12_timer(6);
            print!("KERNEL: System Timer initialised\n");
        }
        {
            unsafe {
                asm!("enable");
            }
            print!("KERNEL: Interrupts enabled\n");
        }

        {
            // initialise the bump allocator
            static mut MEMORY: [u8; MEMORY_SIZE] = [0; MEMORY_SIZE];
            unsafe {
                let allocp = ALLOC.get() as *mut BumpAllocator;
                allocp.write(BumpAllocator::new(&mut MEMORY));
            }
            // force the `allocp` write to complete before returning from this function
            atomic::compiler_fence(Ordering::Release);
            print!("KERNEL: Bump allocator initialised\n");
        }

        {
            const HEAP_SIZE: usize = 1024 * 1024;
            static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
            unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
            print!("KERNEL: Global allocator initialised\n");
        }

        // print!("KERNEL: Initialised\n");
    }

    unsafe fn thread_executor_ref() -> &'static ThreadExecutor {
        Self::get_executors().thread_executor()
    }
}

impl Kernel {
    fn modify_alarm_queue<R>(
        f: impl FnOnce(&mut AlarmQueue<N_ALARM_HANDLES, N_ALARM_ITEMS>) -> R,
    ) -> R {
        let r = unsafe {
            // SAFETY: a core can only call its own `Core` object
            // and interrupts are disabled, so no preemption
            &mut *ALARM_QUEUE.as_mut_ptr()
        };
        f(r)
    }

    fn get_executors() -> &'static ExecutorList<CriticalSectionRawMutex, N_EXECUTORS> {
        unsafe { INTERRUPT_EXECUTORS.borrow() }
    }

    pub fn allocate_interrupt_executor(node: SoftwareInterruptNode) -> ExecutorHandle {
        // print!("allocate_interrupt_executor called with {:#?}\n", node);
        let executors = Self::get_executors();
        // print!("Got executor list {:#?}", executors);
        match executors.allocate(node) {
            Some(x) => x,
            None => panic!("Cpu::allocate_interrupt_executor: executor list unable to allocate\n"),
        }
    }

    pub fn spawn_executor(
        handle: ExecutorHandle,
        fut: impl core::future::Future<Output = ()> + 'static,
        name: &'static str,
    ) {
        print!(
            "KERNEL: Spawned {} at priority {}\n",
            name,
            handle.get_prio()
        );
        Self::get_executors().spawn(handle, fut, name)
    }

    /// This is called inside the [`install_interrupt_executor_handler`] macro
    #[allow(unused)]
    pub fn get_executor(handle: ExecutorHandle) -> &'static InterruptExecutor {
        let x = Self::get_executors().get(handle);
        // hacked: make static lifetime
        unsafe {
            // safety: here, INTERRUPT_EXECUTORS is a static variable, it uses a static hash map to store the
            // data
            &*x
        }
    }

    pub fn start_interrupt_executor(handle: ExecutorHandle) {
        print!(
            "KERNEL: Start interrupt executor at prio {:?}\n",
            handle.get_prio()
        );
        Self::get_executors().start_interrupt_executor(handle);
    }

    pub fn current_tick() -> u32 {
        TICKS.load(Ordering::Relaxed)
    }
}

// alarm queue
const N_ALARM_HANDLES: usize = 8;
const N_ALARM_ITEMS: usize = 64;

static mut ALARM_QUEUE: MaybeUninit<AlarmQueue<N_ALARM_HANDLES, N_ALARM_ITEMS>> =
    MaybeUninit::uninit();

// interrupt executors
const N_EXECUTORS: usize = 4;
static mut INTERRUPT_EXECUTORS: ExecutorList<CriticalSectionRawMutex, N_EXECUTORS> =
    ExecutorList::new();

// timer interrupt
#[no_mangle]
extern "C" fn __INTERRUPT_HANDLER_6() {
    // let gpio00 = pac::P00.split();
    // let mut p00 = gpio00.p00_0.into_push_pull_output();
    // p00.set_high();
    Kernel::on_timer_interrupt();
    // p00.set_low();
}

#[macro_export]
macro_rules! install_interrupt_executor_handler {
    ($prio:expr) => {
        paste::paste! {
            #[no_mangle]
            extern "C" fn [<__INTERRUPT_HANDLER_$prio>]() {
                let handle = unsafe { crate::cpu::executor_list::ExecutorHandle::from_prio($prio) };
                let executor = Kernel::get_executor(handle);
                executor.on_interrupt();
            }
        }
    };
}

// #[no_mangle]
// extern "C" fn __INTERRUPT_HANDLER_2() {
//     let handle = unsafe { ExecutorHandle::from_prio(2) };
//     let executor = Kernel::get_executor(handle);
//     executor.on_interrupt();
// }

// #[no_mangle]
// extern "C" fn __INTERRUPT_HANDLER_3() {
//     let handle = unsafe { ExecutorHandle::from_prio(3) };
//     let executor = Kernel::get_executor(handle);
//     executor.on_interrupt();
// }
