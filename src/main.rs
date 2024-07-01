#![no_main]
#![no_std]

use bw_r_drivers_tc37x as drivers;
use core::arch::asm;
use core::sync::atomic::{compiler_fence, AtomicU32, Ordering};
use core::time::Duration;
use cpu::cpu0::Kernel;
use cpu::Cpu;
use critical_section::RawRestoreState;
use drivers::gpio::{gpio00, GpioExt, Output, Pin};
use drivers::interrupt::SoftwareInterruptNode;
use drivers::scu::wdt::{disable_cpu_watchdog, disable_safety_watchdog};
use drivers::scu::wdt_call::call_without_endinit;
use drivers::uart::init_uart_io;
use drivers::uart::print;
use drivers::{pac, ssw};
use sync::blocking_mutex::CriticalSectionRawMutex;
use sync::pipe::Pipe;
use sync::pubsub::PubSubChannel;
use sync::select::select;
use sync::signal::Signal;

extern crate alloc;

use crate::time::timer_fut::Timer;

mod cpu;
mod executor;
mod memory;
mod runtime;
mod srp_mutex;
mod sync;
mod time;
mod utils;
mod petri_net;

install_interrupt_executor_handler!(2);
install_interrupt_executor_handler!(3);
install_interrupt_executor_handler!(4);

#[export_name = "main"]
fn main() -> ! {
    init_uart_io();
    print!("KERNEL: UART driver initialised\n");
    Kernel::init();
    loop {}
}

// -------------------------------------------------------------------------------------

// #[no_mangle]
// extern "C" fn __INTERRUPT_HANDLER_2() {
//     let handle = unsafe { crate::cpu::executor_list::ExecutorHandle::from_prio(2) };
//     let executor = Cpu0::get_executor(handle);
//     print!(".");
//     executor.on_interrupt();
// }


/// Wait for a number of cycles roughly calculated from a duration.
#[inline(always)]
#[allow(unused)]
fn wait_nop(period: Duration) {
    let ns: u32 = period.as_nanos() as u32;
    let n_cycles = ns / 920;
    for _ in 0..n_cycles {
        // SAFETY: nop is always safe
        // compiler_fence(core::sync::atomic::Ordering::AcqRel);
        unsafe { core::arch::asm!("nop") };
    }
}

// Note: without this, the watchdog will reset the CPU
#[export_name = "Crt0PreInit"]
fn pre_init_fn() {
    let cpu_core_id: u32;
    unsafe {
        core::arch::asm!("mfcr {0}, 0xFE1C", out(reg32) cpu_core_id);
    }
    if cpu_core_id == 0 {
        disable_safety_watchdog();
    }
    disable_cpu_watchdog();
}

#[export_name = "Crt0PostInit"]
fn post_init_fn() {
    // added: park unused CPUs?
    let cpu_core_id: u32;
    unsafe {
        core::arch::asm!("mfcr {0}, 0xFE1C", out(reg32) cpu_core_id);
    }
    if cpu_core_id == 0 {
        if ssw::init_clock().is_err() {
            panic!("Error in ssw init");
        }
        load_interrupt_table();
    } else {
        loop {}
    }
}

#[allow(unused_variables)]
#[panic_handler]
fn panic(panic: &core::panic::PanicInfo<'_>) -> ! {
    defmt::error!("Panic! {}", defmt::Display2Format(panic));
    print!("Panic! {}", panic);
    #[allow(clippy::empty_loop)]
    loop {}
}

struct Section;

critical_section::set_impl!(Section);

// workaround since reading ICR.IE does not work heres...
// maybe because of privileged access?
static NESTED_INTERRUPT_NR: AtomicU32 = AtomicU32::new(0);

unsafe impl critical_section::Impl for Section {
    unsafe fn acquire() -> RawRestoreState {
        // let cpu0 = bw_r_drivers_tc37x::pac::CPU0;
        // let was_enabled = cpu0.icr().read().ie().get();
        // print!("[");
        unsafe {
            asm!("disable");
        }
        // true
        NESTED_INTERRUPT_NR.fetch_add(1, Ordering::Acquire) == 0
    }

    unsafe fn release(was_active: RawRestoreState) {
        // print!("]");
        // if was_active {
        if NESTED_INTERRUPT_NR.fetch_sub(1, Ordering::Release) <= 1 {
            // print!("!");
            unsafe { asm!("enable") }
        }
    }
}

extern "C" {
    static __INTERRUPT_TABLE: u8;
}

fn load_interrupt_table() {
    call_without_endinit(|| unsafe {
        let interrupt_table = &__INTERRUPT_TABLE as *const u8 as u32;
        asm!("mtcr	$biv, {0}", in(reg32) interrupt_table);
        asm!("isync");
    });
}
