#![no_main]
#![no_std]

use bw_r_drivers_tc37x as drivers;
use core::arch::asm;
use core::time::Duration;
use cpu::cpu0::Cpu0;
use critical_section::RawRestoreState;
use drivers::scu::wdt::{disable_cpu_watchdog, disable_safety_watchdog};
use drivers::scu::wdt_call::call_without_endinit;
use drivers::ssw;
use drivers::uart::init_uart_io;
use drivers::uart::print;

use crate::cpu::Cpu;
use crate::time::timer_fut::Timer;

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

    Cpu0::init();

    Cpu0::spawn(
        async {
            loop {
                print!("Task A: {}\n", Cpu0::now());
                Timer::after_ticks(5).await;
            }
        },
        "Task A",
    );

    Cpu0::spawn(
        async {
            loop {
                print!("Task B: {}\n", Cpu0::now());
                Timer::after_ticks(3).await;

                print!("Task B: {}\n", Cpu0::now());
                Timer::after_ticks(3).await;
            }
        },
        "Task B",
    );

    Cpu0::start_executor();
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
