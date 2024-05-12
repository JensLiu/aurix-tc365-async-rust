//! Simple CAN example.

#![no_main]
#![no_std]

use bw_r_drivers_tc37x as drivers;
use core::arch::asm;
use core::time::Duration;
use cpu::core0::Core0;
use critical_section::RawRestoreState;
use drivers::gpio::GpioExt;
use drivers::scu::wdt::{disable_cpu_watchdog, disable_safety_watchdog};
use drivers::scu::wdt_call::call_without_endinit;
use drivers::uart::init_uart_io;
use drivers::uart::print;
use drivers::{pac, ssw};
use executor::yields;

use crate::cpu::Core;

mod cpu;
mod executor;
mod memory;
mod mutex;
mod runtime;
mod time;
mod utils;

#[export_name = "main"]
fn main() -> ! {
    init_uart_io();
    print("Start async executor test\n");
    time::init();
    print("Enable interrupts\n");
    unsafe {
        asm!("enable");
    }

    Core0::spawn(async {
        loop {
            print("Task A: Hello World!\n");
            Core0::yields().await;
        }
    });

    Core0::spawn(async {
        loop {
            print("Task B: Hallo Welt\n");
            Core0::yields().await;

            print("Task B: will yield\n");
            Core0::yields().await;

            print("Task B: will yield again\n");
            Core0::yields().await;

            print("Task B: will yield yet again\n");
            Core0::yields().await;
        }
    });

    Core0::start_executor();
}

/// Wait for a number of cycles roughly calculated from a duration.
#[inline(always)]
fn wait_nop(period: Duration) {
    let ns: u32 = period.as_nanos() as u32;
    let n_cycles = ns / 920;
    for _ in 0..n_cycles {
        // SAFETY: nop is always safe
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
    if ssw::init_clock().is_err() {
        panic!("Error in ssw init");
    }

    load_interrupt_table();
}

#[allow(unused_variables)]
#[panic_handler]
fn panic(panic: &core::panic::PanicInfo<'_>) -> ! {
    defmt::error!("Panic! {}", defmt::Display2Format(panic));
    #[allow(clippy::empty_loop)]
    loop {}
}

struct Section;

critical_section::set_impl!(Section);

unsafe impl critical_section::Impl for Section {
    unsafe fn acquire() -> RawRestoreState {
        unsafe { asm!("disable") };
        true
    }

    unsafe fn release(token: RawRestoreState) {
        if token {
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
