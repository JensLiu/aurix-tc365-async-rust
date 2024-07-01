use futures_util::Future;

use crate::{
    cpu::{alarm::AlarmHandle, cpu0::Kernel, Cpu},
    executor::task::TaskHeader, print,
};

use super::{base_executor::BaseExecutor, pender::Pender};

#[derive(Debug)]
pub struct ThreadExecutor {
    inner: BaseExecutor,
}

impl ThreadExecutor {
    pub const fn new(alarm_handle: Option<AlarmHandle>) -> Self {
        Self {
            inner: BaseExecutor::new(Pender::ThreadExecutorPender, alarm_handle),
        }
    }

    pub fn register_alarm(&self, handle: AlarmHandle) {
        self.inner.register_alarm(handle);
    }

    pub fn spawn(&'static self, f: impl Future + 'static, name: &'static str) {
        self.inner.spawn(f, name)
    }

    pub fn start(&self) -> ! {
        self.inner.register_alarm_callback();
        loop {
            if !Kernel::event_fetch_and_clear_local() {
                // <- possible lost wakeups????
                // If events arrive here, it will not be lost
                // and will be noficed in the next iteration of this buzy loop.
                continue;
            }

            // print!("[debug]: executor woken up\n");
            //     self.check_timer_queue();
            //     self.advance_runnable_tasks();
            //     self.renew_alarm();
            self.inner.exec_once();
        }
    }
    pub unsafe fn enqueue_and_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        self.inner.enqueue_and_pend(task)
    }

    pub unsafe fn enqueue_no_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        self.inner.enqueue_no_pend(task)
    }
}
