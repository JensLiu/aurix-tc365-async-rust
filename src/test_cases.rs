
const PIPE_BUF_SIZE: usize = 32;
static DATAPIPE: Pipe<CriticalSectionRawMutex, PIPE_BUF_SIZE> = Pipe::new();

#[allow(unused)]
fn multi_prio_test() {
    let gpio00 = pac::P00.split();
    let mut led1 = gpio00.p00_5.into_push_pull_output();
    let mut led2 = gpio00.p00_6.into_push_pull_output();

    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));
    let prio3_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 1, 3, 0));
    let prio4_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 2, 4, 0));

    Kernel::spawn_executor(
        prio2_handle,
        async {
            let random_words = ["apple", "mountain", "river", "elephant", "galaxy"];
            loop {
                let now = Kernel::now();
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

    Kernel::spawn_executor(
        prio4_handle,
        async move {
            let random_words = ["breeze", "galore", "zenith", "echo", "luminous"];
            loop {
                let now = Kernel::now();
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

    Kernel::spawn(
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

    Kernel::spawn_executor(
        prio2_handle,
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

    Kernel::spawn_executor(
        prio3_handle,
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

    Kernel::start_interrupt_executor(prio4_handle);
    Kernel::start_interrupt_executor(prio3_handle);
    Kernel::start_interrupt_executor(prio2_handle);
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn interrupt_executor_context_switch_test() {
    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));

    Kernel::spawn_executor(
        prio2_handle,
        {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            async move {
                loop {
                    pin.toggle();
                    Kernel::yields().await;
                    // Timer::after_ticks(1).await;
                }
            }
        },
        "Task Off",
    );

    Kernel::spawn_executor(
        prio2_handle,
        {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            async move {
                loop {
                    pin.toggle();
                    Kernel::yields().await;
                    // Timer::after_ticks(1).await;
                }
            }
        },
        "Task On",
    );
    Kernel::start_interrupt_executor(prio2_handle);
    loop {}
}

// static MUTEX: Mutex<CriticalSectionRawMutex, ()> = Mutex::new(());

#[allow(unused)]
fn thread_executor_context_switch_test() {
    Kernel::spawn(
        {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            async move {
                loop {
                    pin.toggle();
                    // Timer::after_ticks(1).await;
                    Kernel::yields().await;
                }
            }
        },
        "Task Off",
    );

    Kernel::spawn(
        {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            async move {
                loop {
                    pin.toggle();
                    // Timer::after_ticks(1).await;
                    Kernel::yields().await;
                }
            }
        },
        "Task On",
    );
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn preemption_interrupt_executors_test() {
    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));
    let prio3_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 1, 3, 0));
    let prio4_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 2, 4, 0));

    Kernel::spawn_executor(
        prio2_handle,
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            Timer::after_ticks(100).await;
            loop {
                pin.toggle();
            }
        },
        "Prio 2 Task",
    );

    Kernel::spawn_executor(
        prio3_handle,
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            Timer::after_ticks(100).await;
            loop {
                // pin.toggle();
                // wait_nop(Duration::from_millis(1));
                pin.toggle();
                Timer::after_ticks(2).await;
            }
        },
        "Prio 3 Task",
    );

    // Cpu0::spawn_executor(
    //     prio4_handle,
    //     async {
    //         let gpio00 = pac::P00.split();
    //         let mut pin = gpio00.p00_3.into_push_pull_output();
    //         loop {
    //             pin.toggle();
    //             Timer::after_ticks(1).await;
    //         }
    //     },
    //     "Prio 4 Task",
    // );

    Kernel::start_interrupt_executor(prio2_handle);
    Kernel::start_interrupt_executor(prio3_handle);
    // Cpu0::start_interrupt_executor(prio4_handle);
    loop {}
}

#[allow(unused)]
fn preemption_thread_interrupt_executors_test() {
    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));

    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            Timer::after_ticks(100).await;
            loop {
                pin.toggle();
            }
        },
        "Thread Task",
    );

    Kernel::spawn_executor(
        prio2_handle,
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            Timer::after_ticks(100).await;
            loop {
                pin.toggle();
                Timer::after_ticks(1).await;
            }
        },
        "Prio 2 Task",
    );

    Kernel::start_interrupt_executor(prio2_handle);
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn pin_switch_baseline() {
    let gpio00 = pac::P00.split();
    let mut pin = gpio00.p00_1.into_push_pull_output();
    loop {
        pin.toggle();
    }
}

#[allow(unused)]
fn starvation_test() {
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            loop {
                pin.toggle();
                Kernel::yields().await;
            }
        },
        "Fast Task",
    );
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            loop {
                pin.toggle();
                Timer::after_ticks(1).await;
            }
        },
        "Slow Task",
    );
    Kernel::start_thread_executor();
}

static PIPE: Pipe<CriticalSectionRawMutex, 1> = Pipe::new();

#[allow(unused)]
fn inter_task_communication_multi_prio_interrupt() {
    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));
    let prio3_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 1, 3, 0));

    Kernel::spawn_executor(
        prio2_handle,
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            loop {
                pin.toggle();
                PIPE.write_all("a".as_bytes()).await;
            }
        },
        "Sender Task",
    );

    Kernel::spawn(
        async move {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            let mut buf: [u8; 1] = [0; 1];
            loop {
                pin.toggle();
                PIPE.read(&mut buf).await;
                print!("Waiter Task: wait for signal\n");
            }
        },
        "Waiter Task",
    );

    Kernel::start_interrupt_executor(prio2_handle);
    // Cpu0::start_interrupt_executor(prio3_handle);
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn inter_task_signal_thread() {
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            loop {
                pin.toggle();
                PIPE.write_all("a".as_bytes()).await;
            }
        },
        "Sender Task",
    );

    Kernel::spawn(
        async move {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            let mut buf: [u8; 1] = [0; 1];
            loop {
                pin.toggle();
                PIPE.read(&mut buf).await;
                print!("Waiter Task: wait for signal\n");
            }
        },
        "Waiter Task",
    );

    // Cpu0::start_interrupt_executor(prio2_handle);
    // Cpu0::start_interrupt_executor(prio3_handle);
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn inter_task_communication_thread() {

    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            loop {
                pin.toggle();
                PIPE.write_all("a".as_bytes()).await;
            }
        },
        "Sender Task",
    );

    Kernel::spawn(
        async move {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            let mut buf: [u8; 1] = [0; 1];
            loop {
                pin.toggle();
                PIPE.read(&mut buf).await;
                print!("Waiter Task: wait for signal\n");
            }
        },
        "Waiter Task",
    );

    // Cpu0::start_interrupt_executor(prio2_handle);
    // Cpu0::start_interrupt_executor(prio3_handle);
    Kernel::start_thread_executor();
}


#[allow(unused)]
fn meassure_frequency() {
    let gpio00 = pac::P00.split();
    let mut pin = gpio00.p00_1.into_push_pull_output();
    print!("begin\n");
    pin.toggle();
    for _ in 0..10_000 {
        unsafe { core::arch::asm!("nop") };
    }
    pin.toggle();
    print!("end\n");
}

#[allow(unused)]
fn thread_executor_multiple_timers() {
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_1.into_push_pull_output();
            loop {
                pin.toggle();
                // print!("1");
                Timer::after_ticks(1).await;
            }
        },
        "Task 1",
    );
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_2.into_push_pull_output();
            loop {
                pin.toggle();
                // print!("2");
                Timer::after_ticks(1).await;
            }
        },
        "Task 2",
    );
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_3.into_push_pull_output();
            loop {
                pin.toggle();
                // print!("3");
                Timer::after_ticks(1).await;
            }
        },
        "Task 3",
    );
    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut pin = gpio00.p00_5.into_push_pull_output();
            loop {
                pin.toggle();
                // print!("5");
                Timer::after_ticks(1).await;
            }
        },
        "Task 5",
    );
    Kernel::start_thread_executor();
}

static TASK_C_PUB_CHANNEL: PubSubChannel<CriticalSectionRawMutex, u32, 16, 2, 1> =
    PubSubChannel::new();
static SIGNAL_A: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static SIGNAL_B: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[allow(unused)]
fn test() {
    let prio2_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 0, 2, 0));
    let prio3_handle = Kernel::allocate_interrupt_executor(SoftwareInterruptNode::new(0, 1, 3, 0));

    Kernel::spawn_executor(
        prio2_handle,
        async {
            let mut sub = TASK_C_PUB_CHANNEL.subscriber().unwrap();
            loop {
                SIGNAL_A.signal(());
                print!("Task A: wait for message\n");
                match sub.next_message().await {
                    sync::pubsub::WaitResult::Lagged(_) => print!("Task A: lagged behind\n"),
                    sync::pubsub::WaitResult::Message(x) => {
                        print!("Task A: message from Task C {:?}\n", x)
                    }
                }
                let tick = Kernel::current_tick() as u64;
                Timer::after_ticks(tick % 100 + tick % 70).await;
            }
        },
        "Task A",
    );

    Kernel::spawn_executor(
        prio2_handle,
        async move {
            let mut sub = TASK_C_PUB_CHANNEL.subscriber().unwrap();
            loop {
                SIGNAL_B.signal(());
                print!("Task B: wait for message\n");
                match sub.next_message().await {
                    sync::pubsub::WaitResult::Lagged(_) => print!("Task B: lagged behind\n"),
                    sync::pubsub::WaitResult::Message(x) => {
                        print!("Task B: message from Task C {:?}\n", x)
                    }
                }
                let tick = Kernel::current_tick() as u64;
                Timer::after_ticks(tick % 100 + tick & 60).await;
            }
        },
        "Task B",
    );

    Kernel::spawn_executor(
        prio3_handle,
        async move {
            let publisher = TASK_C_PUB_CHANNEL.publisher().unwrap();
            let mut counter: u32 = 0;
            loop {
                print!("Task C: await signal\n");
                select(SIGNAL_A.wait(), SIGNAL_B.wait()).await;
                print!("Task C: received signal\n");
                counter += 1;
                print!("Task C: try publish message: {:?}\n", counter);
                publisher.publish(counter).await;
            }
        },
        "Task C",
    );

    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut led1 = gpio00.p00_5.into_push_pull_output();
            loop {
                print("Task D: Blink LED 1\n");
                led1.toggle();
                Timer::after_ticks(50).await;
            }
        },
        "Task D",
    );

    Kernel::spawn(
        async {
            let gpio00 = pac::P00.split();
            let mut led2 = gpio00.p00_6.into_push_pull_output();
            loop {
                print!("Task E: Blink LED 2\n");
                led2.toggle();
                Timer::after_ticks(100).await;
            }
        },
        "Task E",
    );

    Kernel::start_interrupt_executor(prio2_handle);
    Kernel::start_interrupt_executor(prio3_handle);
    Kernel::start_thread_executor();
}

#[allow(unused)]
fn test_uart_transmission_time() {
    let gpio00 = pac::P00.split();
    let mut pin = gpio00.p00_1.into_push_pull_output();
    loop {
        pin.toggle();
        print!("hello world\n");
        pin.toggle();
        print!("hello world, hello world\n");
        pin.toggle();
        print!("hello world, hello world, hello word\n");
        pin.toggle();
        print!("hello world, hello world, hello world, hello world\n");
        pin.toggle();
        print!("hello world, hello world, hello word, hello world, hello world\n");
    }
}

fn allocator_test() {
    use alloc::vec::Vec;
    let mut v: Vec<u8> = Vec::new();
    for i in 0..100 {
        print!("allocate {}\n", i);
        v.push(i as u8);
    }
    for i in v {
        print!("{}, ", i);
    }
    print!("\n");
}