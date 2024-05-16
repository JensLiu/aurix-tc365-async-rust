pub mod alarm;
pub mod cpu0;

use core::future::Future;

use self::alarm::AlarmHandle;
use crate::executor::{executor_impl::Executor, yields::Yielder};

pub trait Cpu {
    const CPU_NR: u8;

    fn init();

    fn signal_event_local();

    fn event_fetch_and_clear_local() -> bool;

    fn is_current_core() -> bool {
        let cpu_core_id: u32;
        unsafe {
            core::arch::asm!("mfcr {0}, 0xFE1C", out(reg32) cpu_core_id);
        }
        cpu_core_id == Self::CPU_NR as u32
    }

    fn on_timer_interrupt();

    fn executor_ref() -> &'static mut Executor
    where
        Self: Sized;

    fn spawn(fut: impl Future<Output = ()> + 'static, name: &'static str);

    fn start_executor() -> !;

    fn yields() -> Yielder<Self>
    where
        Self: Sized;

    // about timer
    fn now() -> u64;

    fn allocate_alarm() -> Option<AlarmHandle>;

    fn set_alarm_callback(handle: AlarmHandle, callback: fn(*mut ()), context: *mut ());

    fn set_alarm(handle: AlarmHandle, tick: u64) -> Option<()>;
}
