#![no_main]
#![no_std]

use bw_r_drivers_tc37x as drivers;
use core::arch::asm;
use core::time::Duration;
use cpu::cpu0::Cpu0;
use critical_section::RawRestoreState;
use drivers::gpio::GpioExt;
use drivers::scu::wdt::{disable_cpu_watchdog, disable_safety_watchdog};
use drivers::scu::wdt_call::call_without_endinit;
use drivers::uart::init_uart_io;
use drivers::uart::print;
use drivers::{pac, ssw};
use sync::blocking_mutex::CriticalSectionRawMutex;
use sync::mutex::Mutex;
use sync::pipe::Pipe;

use crate::cpu::Cpu;
use crate::time::timer_fut::Timer;

mod cpu;
mod executor;
mod memory;
mod runtime;
mod sync;
mod time;
mod utils;

const PIPE_BUF_SIZE: usize = 32;
static DATAPIPE: Pipe<CriticalSectionRawMutex, PIPE_BUF_SIZE> = Pipe::new();

#[export_name = "main"]
fn main() -> ! {
    init_uart_io();

    Cpu0::init();

    let gpio00 = pac::P00.split();
    let mut led1 = gpio00.p00_5.into_push_pull_output();
    let mut led2 = gpio00.p00_6.into_push_pull_output();

    Cpu0::spawn(
        async {
            let random_words = ["apple", "mountain", "river", "elephant", "galaxy"];
            loop {
                let now = Cpu0::now();
                let idx = now as usize % random_words.len();
                let word = random_words[idx];
                let mut buf = [0u8; PIPE_BUF_SIZE];
                let s =
                    format_no_std::show(&mut buf, format_args!("From Task A: {}\n", word)).unwrap();
                DATAPIPE.write_all(s.as_bytes()).await;
                Timer::after_ticks(1000).await;
            }
        },
        "Task A",
    );

    Cpu0::spawn(
        async move {
            let random_words = ["breeze", "galore", "zenith", "echo", "luminous"];
            loop {
                let now = Cpu0::now();
                let idx = now as usize % random_words.len();
                let word = random_words[idx];
                let mut buf = [0u8; PIPE_BUF_SIZE];
                let s =
                    format_no_std::show(&mut buf, format_args!("From Task B: {}\n", word)).unwrap();
                DATAPIPE.write_all(s.as_bytes()).await;
                Timer::after_ticks(200).await;
            }
        },
        "Task B",
    );

    Cpu0::spawn(
        async move {
            loop {
                let mut buf = [0u8; PIPE_BUF_SIZE];
                DATAPIPE.read(&mut buf).await;
                let s = core::str::from_utf8(&buf).unwrap();
                print!("{}", s);
            }
        },
        "Task C",
    );

    Cpu0::spawn(
        async move {
            loop {
                led1.set_high();
                Timer::after_ticks(50).await;
                led1.set_low();
                Timer::after_ticks(50).await;
            }
        },
        "Task D",
    );

    Cpu0::spawn(
        async move {
            loop {
                led2.set_high();
                Timer::after_ticks(150).await;
                led2.set_low();
                Timer::after_ticks(150).await;
            }
        },
        "Task E",
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
    print!("Panic! {}", panic);
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
