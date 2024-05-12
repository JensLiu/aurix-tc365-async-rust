use core::sync::atomic::{AtomicU32, Ordering};

use bw_r_drivers_tc37x::uart::print;

use crate::cpu::{core0::Core0, Core};

mod timer;

static TICKS: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
extern "C" fn __INTERRUPT_HANDLER_6() {
    Core0::on_timer_interrupt();
}

pub fn init() {
    bw_r_drivers_tc37x::timer::init_gpt12_timer();
    print("timer init\n");
}