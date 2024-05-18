use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    pin::Pin,
    task::{self, Context, RawWaker, Waker},
};

use heapless::{Deque, Vec};

use crate::{
    cpu::{alarm::AlarmHandle, cpu0::Cpu0, Cpu},
    print,
    sync::blocking_mutex::{CriticalSectionRawMutex, Mutex},
    time::timer_queue::{TimerQueue, TimerQueueItem},
};

use super::{task::TaskHeader, waker::VTABLE};

const MAX_TASKS: usize = 32;

pub struct Executor {
    // not using mutex for now we are only testing on a single core and it will
    // only be mutated in thread mode, hence no preemption
    run_queue: Mutex<CriticalSectionRawMutex, Deque<&'static mut TaskHeader, MAX_TASKS>>,
    timer_queue: Mutex<CriticalSectionRawMutex, TimerQueue>,
    alarm_handle: Option<AlarmHandle>,
    next_alarm_at: UnsafeCell<Option<u64>>,
}

impl Executor {
    pub fn new(alarm_handle: Option<AlarmHandle>) -> Self {
        Self {
            run_queue: Mutex::new(Deque::new()),
            timer_queue: Mutex::new(TimerQueue::new()),
            alarm_handle,
            next_alarm_at: UnsafeCell::new(None),
        }
    }

    pub fn spawn(&'static self, f: impl Future + 'static, name: &'static str) {
        if self
            .run_queue
            .lock_mut(|x| x.push_front(TaskHeader::new(f, self, name)))
            .is_err()
        {
            panic!("Executor::spawn: FAILED\n");
            // crate::utils::abort();
        }
    }

    pub fn start(&self) -> ! {
        self.register_alarm_callback();
        loop {
            if !Cpu0::event_fetch_and_clear_local() {
                // <- possible lost wakeups????
                // If events arrive here, it will not be lost
                // and will be noficed in the next iteration of this buzy loop.
                continue;
            }

            // print!("[debug]: executor woken up\n");
            self.check_timer_queue();
            self.advance_runnable_tasks();
            self.renew_alarm();
        }
    }

    fn check_timer_queue(&self) {
        let now = Cpu0::now();
        let mut ready: Vec<&mut TaskHeader, MAX_TASKS> = Vec::new();

        self.timer_queue.lock_mut(|tq| {
            tq.dequeue_expired(now, |item| {
                let task_ref = unsafe { item.get_task_ref().as_static_mut_header() };
                ready.push(task_ref).ok().unwrap();
            })
        });

        self.run_queue.lock_mut(|x| {
            for task in ready {
                x.push_front(task).ok().unwrap();
            }
        });

        // print!("[debug] expired tasks at {}: {}\n", now, n_expired);
    }

    fn update_timer_queue(&self, task: &'static mut TaskHeader) {
        if task.expires_at.is_some() {
            // print!(
            //     "[debug]: Executor::updpate_timer_queue: task {} timer was set to {}\n",
            //     task.name,
            //     task.expires_at.unwrap()
            // );
            self.timer_queue.lock_mut(|x| {
                let expires_at = task.expires_at.unwrap();
                let waker = unsafe {
                    Waker::from_raw(RawWaker::new(task as *const _ as *const _, &VTABLE))
                };
                x.enqueue(TimerQueueItem { waker, expires_at });
            })
        }
    }

    fn renew_alarm(&self) {
        if let Some(next_tick) = self.timer_queue.lock(|x| x.next_expiration()) {
            // print!(
            //     "[debug]: Executor::renew_alarm: set to tick at {}\n",
            //     next_tick
            // );
            self.set_alarm(next_tick);
        }
    }

    // public interface
    pub unsafe fn enqueue_and_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        // SAFETY: make sure that there is one instance of a TaskHeader reference
        // in the run_queue and the timer_queue
        let x = self.run_queue.lock_mut(|x| x.push_back(task)).ok();
        Cpu0::signal_event_local();
        x
    }

    pub unsafe fn enqueue_no_pend(&self, task: &'static mut TaskHeader) -> Option<()> {
        self.run_queue.lock_mut(|x| x.push_back(task)).ok()
    }

    // task execution ------------------------------------------------------------------------------------------------------

    fn advance_runnable_tasks(&self) {
        // set to suspended because it has been polled by `pop`-ing it out of the `ready_queue`
        while let Some(task_ref) = self.run_queue.lock_mut(|x| x.pop_front()) {
            task_ref.expires_at = None; // clear its expiration bit because it has already been woken

            // create waker and context for this task on the fly
            // to pass it to the `Future::poll` chain
            let waker = unsafe {
                Waker::from_raw(RawWaker::new(
                    task_ref as *const TaskHeader as *const (), // as argument to the `Waker::wake` function
                    &VTABLE,
                ))
            };
            let mut cx = Context::from_waker(&waker);

            // poll the future: one execution of the state machine
            let _ = unsafe {
                // SAFETY: It points to a static memory location, so it is naturally Pin-ed
                let fut = Pin::new_unchecked(&mut *task_ref.fut_ref);
                fut.poll(&mut cx).is_ready()
            };

            // some timer might be set while executing, check if it needs enqueuing
            self.update_timer_queue(task_ref);
        }
    }

    // alarm action -----------------------------------------------------------------------------------

    /// The executor registers an alarm. If it expires, the alarm_callback function will be called
    /// which the context passed being a pointer to itself.
    pub fn register_alarm_callback(&self) {
        if self.alarm_handle.is_some() {
            Cpu0::set_alarm_callback(
                self.alarm_handle.unwrap(),
                Self::alarm_callback,
                self as *const _ as *mut (),
            );
        }
    }

    /// Set the next tick when the alarm will kick off
    pub fn set_alarm(&self, expires_at: u64) -> Option<()> {
        // match unsafe { &mut *self.next_alarm_at.get() } {
        //     Some(next) if *next <= expires_at => None,
        //     next => {
        //         // next > expires_at
        //         // has newer timer, update the alarm
        //         // though it does not remove the originally registered alarm and may have duplicate
        //         // ones as it may get set again, it reduces the duplicatoin
        //         // TODO: find an alternative solution or just leave it?
        //         *next = Some(expires_at);
        //         Cpu0::set_alarm(self.alarm_handle.unwrap(), expires_at) // set the next tick
        //     }
        // }
        Cpu0::set_alarm(self.alarm_handle.unwrap(), expires_at) // set the next tick
    }

    /// Callback function when the alarm set by set_alarm expires
    pub fn alarm_callback(ctx_as_self: *mut ()) {
        let zelf = unsafe { &mut *(ctx_as_self as *mut Self) };
        // print!("[debug]: timer callback on executor\n");
        Cpu0::signal_event_local(); // some timer expired, wake up the executor (TODO: generalise this)
    }
}
