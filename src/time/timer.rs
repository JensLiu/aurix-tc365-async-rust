use core::{future::Future, sync::atomic::{AtomicU32, Ordering}, task::{Poll, Waker}};

use bw_r_drivers_tc37x::uart::print;

// interrupt handler for the priority 6 task on core 0 (vector table 0)

#[macro_export]
macro_rules! interrupt_handler_gen {
    ($int_nr:expr, $code:block) => {
        paste::psate! {
            #[no_mangle]
            extern "C" fn [<__INTERRUPT_HANDLER_$int_nr>]() {
                $code
            }
        }
    };
    ($int_nr:expr) => {
        paste::paste! {
            #[no_mangle]
            extern "C" fn [<__INTERRUPT_HANDLER_$int_nr>]() {
                print("interrupt being called");
                loop {}
            }
        }
    }
}



// pub struct TimerFut {
//     expires_at: , 
// }

// impl TimerFut {
//     pub fn has_expired(&self) -> bool {
//         TICKS.load(Ordering::SeqCst) >= self.at_ticks
//     }
// }

// impl Future for TimerFut {
//     type Output = ();

//     fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
//         if self.has_expired() {
//             cx.waker().wake_by_ref();
//             core_0_signal_event();
//             Poll::Ready(())
//         } else {
//             Poll::Pending
//         }
//     }
// }

#[derive(Debug)]
struct TimerMeta {
    at: u64,
    waker: Waker,
}

impl PartialEq for TimerMeta {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl Eq for TimerMeta {}

impl PartialOrd for TimerMeta {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.at.partial_cmp(&other.at)
    }
}

impl Ord for TimerMeta {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.at.cmp(&other.at)
    }
}

const fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub const TICK_HZ: u64 = 20_000;

pub(crate) const GCD_1K: u64 = gcd(TICK_HZ, 1_000);
pub(crate) const GCD_1M: u64 = gcd(TICK_HZ, 1_000_000);
pub(crate) const GCD_1G: u64 = gcd(TICK_HZ, 1_000_000_000);