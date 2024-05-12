pub mod core0;

use core::future::Future;

use crate::executor::{executor_impl::Executor, yields::Yielder};

pub trait Core {
    const CORE_NR: u8;

    fn signal_event_local();

    fn event_fetch_and_clear_local() -> bool;

    fn is_current_core() -> bool;

    fn on_timer_interrupt();

    fn executor_ref() -> &'static Executor<Self>
    where
        Self: Sized;

    fn spawn(fut: impl Future<Output = ()> + 'static);

    fn start_executor() -> !;

    fn yields() -> Yielder<Self>
    where
        Self: Sized;
}
