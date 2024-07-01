use core::arch::asm;

use bw_r_drivers_tc37x::interrupt::SoftwareInterruptNode;
use futures_util::Future;

use crate::{
    cpu::{
        alarm::{self, AlarmHandle},
        cpu0::Kernel,
        Cpu,
    },
    executor::task::TaskHeader, print,
};

use super::{base_executor::BaseExecutor, pender::Pender};

#[derive(Debug)]
pub struct InterruptExecutor {
    inner: BaseExecutor,
}

impl InterruptExecutor {
    pub const fn new(
        sw_interrupt: SoftwareInterruptNode,
        alarm_handle: Option<AlarmHandle>,
    ) -> Self {
        Self {
            inner: BaseExecutor::new(
                Pender::InterruptExecutorPender { sw_interrupt },
                alarm_handle,
            ),
        }
    }

    pub fn spawn(&'static self, f: impl Future + 'static, name: &'static str) {
        self.inner.spawn(f, name)
    }

    pub fn start(&self) {
        self.inner.register_alarm_callback();
        self.inner.pender.pend();
    }

    pub fn on_interrupt(&self) {
        self.inner.exec_once();
    }

    pub unsafe fn enqueue_and_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        self.inner.enqueue_and_pend(task)
    }

    pub unsafe fn enqueue_no_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        self.inner.enqueue_no_pend(task)
    }
}
