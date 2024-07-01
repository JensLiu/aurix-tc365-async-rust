use bw_r_drivers_tc37x::interrupt::SoftwareInterruptNode;

use crate::{cpu::{cpu0::Kernel, Cpu}, print};

#[derive(Debug)]
pub enum Pender {
    ThreadExecutorPender,
    InterruptExecutorPender { sw_interrupt: SoftwareInterruptNode },
}

impl Pender {
    pub fn pend(&self) {
        match *self {
            Pender::ThreadExecutorPender => {
                // print!("Pender::pend: pended thread executor\n");
                Kernel::signal_event_local();
            }
            Pender::InterruptExecutorPender { sw_interrupt } => {
                // print!("Pender::pend: pended interrupt executor{}\n", sw_interrupt.get_prio());
                sw_interrupt.set_request();
            }
        }
    }
}
