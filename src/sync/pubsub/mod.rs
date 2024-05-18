//! Implementation of [PubSubChannel], a queue where published messages get received by all subscribers.

#![deny(missing_docs)]

use core::cell::RefCell;
use core::fmt::Debug;
use core::task::{Context, Poll};

use heapless::Deque;

use self::publisher::{ImmediatePub, Pub};
use self::subscriber::Sub;
use super::blocking_mutex::RawMutex;
use super::blocking_mutex::Mutex;
use super::waitqueue::MultiWakerRegistration;

pub mod publisher;
pub mod subscriber;

pub use publisher::{DynImmediatePublisher, DynPublisher, ImmediatePublisher, Publisher};
pub use subscriber::{DynSubscriber, Subscriber};

/// A broadcast channel implementation where multiple publishers can send messages to multiple subscribers
///
/// Any published message can be read by all subscribers.
/// A publisher can choose how it sends its message.
///
/// - With [Pub::publish()] the publisher has to wait until there is space in the internal message queue.
/// - With [Pub::publish_immediate()] the publisher doesn't await and instead lets the oldest message
/// in the queue drop if necessary. This will cause any [Subscriber] that missed the message to receive
/// an error to indicate that it has lagged.
///
/// ## Example
///
/// ```
/// # use embassy_sync::blocking_mutex::raw::NoopRawMutex;
/// # use embassy_sync::pubsub::WaitResult;
/// # use embassy_sync::pubsub::PubSubChannel;
/// # use futures_executor::block_on;
/// # let test = async {
/// // Create the channel. This can be static as well
/// let channel = PubSubChannel::<NoopRawMutex, u32, 4, 4, 4>::new();
///
/// // This is a generic subscriber with a direct reference to the channel
/// let mut sub0 = channel.subscriber().unwrap();
/// // This is a dynamic subscriber with a dynamic (trait object) reference to the channel
/// let mut sub1 = channel.dyn_subscriber().unwrap();
///
/// let pub0 = channel.publisher().unwrap();
///
/// // Publish a message, but wait if the queue is full
/// pub0.publish(42).await;
///
/// // Publish a message, but if the queue is full, just kick out the oldest message.
/// // This may cause some subscribers to miss a message
/// pub0.publish_immediate(43);
///
/// // Wait for a new message. If the subscriber missed a message, the WaitResult will be a Lag result
/// assert_eq!(sub0.next_message().await, WaitResult::Message(42));
/// assert_eq!(sub1.next_message().await, WaitResult::Message(42));
///
/// // Wait again, but this time ignore any Lag results
/// assert_eq!(sub0.next_message_pure().await, 43);
/// assert_eq!(sub1.next_message_pure().await, 43);
///
/// // There's also a polling interface
/// assert_eq!(sub0.try_next_message(), None);
/// assert_eq!(sub1.try_next_message(), None);
/// # };
/// #
/// # block_on(test);
/// ```
///
pub struct PubSubChannel<M: RawMutex, T: Clone, const CAP: usize, const SUBS: usize, const PUBS: usize> {
    inner: Mutex<M, RefCell<PubSubState<T, CAP, SUBS, PUBS>>>,
}

impl<M: RawMutex, T: Clone, const CAP: usize, const SUBS: usize, const PUBS: usize>
    PubSubChannel<M, T, CAP, SUBS, PUBS>
{
    /// Create a new channel
    pub const fn new() -> Self {
        Self {
            inner: Mutex::const_new(M::INIT, RefCell::new(PubSubState::new())),
        }
    }

    /// Create a new subscriber. It will only receive messages that are published after its creation.
    ///
    /// If there are no subscriber slots left, an error will be returned.
    pub fn subscriber(&self) -> Result<Subscriber<M, T, CAP, SUBS, PUBS>, Error> {
        self.inner.lock(|inner| {
            let mut s = inner.borrow_mut();

            if s.subscriber_count >= SUBS {
                Err(Error::MaximumSubscribersReached)
            } else {
                s.subscriber_count += 1;
                Ok(Subscriber(Sub::new(s.next_message_id, self)))
            }
        })
    }

    /// Create a new subscriber. It will only receive messages that are published after its creation.
    ///
    /// If there are no subscriber slots left, an error will be returned.
    pub fn dyn_subscriber(&self) -> Result<DynSubscriber<'_, T>, Error> {
        self.inner.lock(|inner| {
            let mut s = inner.borrow_mut();

            if s.subscriber_count >= SUBS {
                Err(Error::MaximumSubscribersReached)
            } else {
                s.subscriber_count += 1;
                Ok(DynSubscriber(Sub::new(s.next_message_id, self)))
            }
        })
    }

    /// Create a new publisher
    ///
    /// If there are no publisher slots left, an error will be returned.
    pub fn publisher(&self) -> Result<Publisher<M, T, CAP, SUBS, PUBS>, Error> {
        self.inner.lock(|inner| {
            let mut s = inner.borrow_mut();

            if s.publisher_count >= PUBS {
                Err(Error::MaximumPublishersReached)
            } else {
                s.publisher_count += 1;
                Ok(Publisher(Pub::new(self)))
            }
        })
    }

    /// Create a new publisher
    ///
    /// If there are no publisher slots left, an error will be returned.
    pub fn dyn_publisher(&self) -> Result<DynPublisher<'_, T>, Error> {
        self.inner.lock(|inner| {
            let mut s = inner.borrow_mut();

            if s.publisher_count >= PUBS {
                Err(Error::MaximumPublishersReached)
            } else {
                s.publisher_count += 1;
                Ok(DynPublisher(Pub::new(self)))
            }
        })
    }

    /// Create a new publisher that can only send immediate messages.
    /// This kind of publisher does not take up a publisher slot.
    pub fn immediate_publisher(&self) -> ImmediatePublisher<M, T, CAP, SUBS, PUBS> {
        ImmediatePublisher(ImmediatePub::new(self))
    }

    /// Create a new publisher that can only send immediate messages.
    /// This kind of publisher does not take up a publisher slot.
    pub fn dyn_immediate_publisher(&self) -> DynImmediatePublisher<T> {
        DynImmediatePublisher(ImmediatePub::new(self))
    }
}

impl<M: RawMutex, T: Clone, const CAP: usize, const SUBS: usize, const PUBS: usize> PubSubBehavior<T>
    for PubSubChannel<M, T, CAP, SUBS, PUBS>
{
    fn get_message_with_context(&self, next_message_id: &mut u64, cx: Option<&mut Context<'_>>) -> Poll<WaitResult<T>> {
        self.inner.lock(|s| {
            let mut s = s.borrow_mut();

            // Check if we can read a message
            match s.get_message(*next_message_id) {
                // Yes, so we are done polling
                Some(WaitResult::Message(message)) => {
                    *next_message_id += 1;
                    Poll::Ready(WaitResult::Message(message))
                }
                // No, so we need to reregister our waker and sleep again
                None => {
                    if let Some(cx) = cx {
                        s.subscriber_wakers.register(cx.waker());
                    }
                    Poll::Pending
                }
                // We missed a couple of messages. We must do our internal bookkeeping and return that we lagged
                Some(WaitResult::Lagged(amount)) => {
                    *next_message_id += amount;
                    Poll::Ready(WaitResult::Lagged(amount))
                }
            }
        })
    }

    fn available(&self, next_message_id: u64) -> u64 {
        self.inner.lock(|s| s.borrow().next_message_id - next_message_id)
    }

    fn publish_with_context(&self, message: T, cx: Option<&mut Context<'_>>) -> Result<(), T> {
        self.inner.lock(|s| {
            let mut s = s.borrow_mut();
            // Try to publish the message
            match s.try_publish(message) {
                // We did it, we are ready
                Ok(()) => Ok(()),
                // The queue is full, so we need to reregister our waker and go to sleep
                Err(message) => {
                    if let Some(cx) = cx {
                        s.publisher_wakers.register(cx.waker());
                    }
                    Err(message)
                }
            }
        })
    }

    fn publish_immediate(&self, message: T) {
        self.inner.lock(|s| {
            let mut s = s.borrow_mut();
            s.publish_immediate(message)
        })
    }

    fn space(&self) -> usize {
        self.inner.lock(|s| {
            let s = s.borrow();
            s.queue.capacity() - s.queue.len()
        })
    }

    fn unregister_subscriber(&self, subscriber_next_message_id: u64) {
        self.inner.lock(|s| {
            let mut s = s.borrow_mut();
            s.unregister_subscriber(subscriber_next_message_id)
        })
    }

    fn unregister_publisher(&self) {
        self.inner.lock(|s| {
            let mut s = s.borrow_mut();
            s.unregister_publisher()
        })
    }
}

/// Internal state for the PubSub channel
struct PubSubState<T: Clone, const CAP: usize, const SUBS: usize, const PUBS: usize> {
    /// The queue contains the last messages that have been published and a countdown of how many subscribers are yet to read it
    queue: Deque<(T, usize), CAP>,
    /// Every message has an id.
    /// Don't worry, we won't run out.
    /// If a million messages were published every second, then the ID's would run out in about 584942 years.
    next_message_id: u64,
    /// Collection of wakers for Subscribers that are waiting.  
    subscriber_wakers: MultiWakerRegistration<SUBS>,
    /// Collection of wakers for Publishers that are waiting.  
    publisher_wakers: MultiWakerRegistration<PUBS>,
    /// The amount of subscribers that are active
    subscriber_count: usize,
    /// The amount of publishers that are active
    publisher_count: usize,
}

impl<T: Clone, const CAP: usize, const SUBS: usize, const PUBS: usize> PubSubState<T, CAP, SUBS, PUBS> {
    /// Create a new internal channel state
    const fn new() -> Self {
        Self {
            queue: Deque::new(),
            next_message_id: 0,
            subscriber_wakers: MultiWakerRegistration::new(),
            publisher_wakers: MultiWakerRegistration::new(),
            subscriber_count: 0,
            publisher_count: 0,
        }
    }

    fn try_publish(&mut self, message: T) -> Result<(), T> {
        if self.subscriber_count == 0 {
            // We don't need to publish anything because there is no one to receive it
            return Ok(());
        }

        if self.queue.is_full() {
            return Err(message);
        }
        // We just did a check for this
        self.queue.push_back((message, self.subscriber_count)).ok().unwrap();

        self.next_message_id += 1;

        // Wake all of the subscribers
        self.subscriber_wakers.wake();

        Ok(())
    }

    fn publish_immediate(&mut self, message: T) {
        // Make space in the queue if required
        if self.queue.is_full() {
            self.queue.pop_front();
        }

        // This will succeed because we made sure there is space
        self.try_publish(message).ok().unwrap();
    }

    fn get_message(&mut self, message_id: u64) -> Option<WaitResult<T>> {
        let start_id = self.next_message_id - self.queue.len() as u64;

        if message_id < start_id {
            return Some(WaitResult::Lagged(start_id - message_id));
        }

        let current_message_index = (message_id - start_id) as usize;

        if current_message_index >= self.queue.len() {
            return None;
        }

        // We've checked that the index is valid
        let queue_item = self.queue.iter_mut().nth(current_message_index).unwrap();

        // We're reading this item, so decrement the counter
        queue_item.1 -= 1;

        let message = if current_message_index == 0 && queue_item.1 == 0 {
            let (message, _) = self.queue.pop_front().unwrap();
            self.publisher_wakers.wake();
            // Return pop'd message without clone
            message
        } else {
            queue_item.0.clone()
        };

        Some(WaitResult::Message(message))
    }

    fn unregister_subscriber(&mut self, subscriber_next_message_id: u64) {
        self.subscriber_count -= 1;

        // All messages that haven't been read yet by this subscriber must have their counter decremented
        let start_id = self.next_message_id - self.queue.len() as u64;
        if subscriber_next_message_id >= start_id {
            let current_message_index = (subscriber_next_message_id - start_id) as usize;
            self.queue
                .iter_mut()
                .skip(current_message_index)
                .for_each(|(_, counter)| *counter -= 1);

            let mut wake_publishers = false;
            while let Some((_, count)) = self.queue.front() {
                if *count == 0 {
                    self.queue.pop_front().unwrap();
                    wake_publishers = true;
                } else {
                    break;
                }
            }

            if wake_publishers {
                self.publisher_wakers.wake();
            }
        }
    }

    fn unregister_publisher(&mut self) {
        self.publisher_count -= 1;
    }
}

/// Error type for the [PubSubChannel]
#[derive(Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// All subscriber slots are used. To add another subscriber, first another subscriber must be dropped or
    /// the capacity of the channels must be increased.
    MaximumSubscribersReached,
    /// All publisher slots are used. To add another publisher, first another publisher must be dropped or
    /// the capacity of the channels must be increased.
    MaximumPublishersReached,
}

/// 'Middle level' behaviour of the pubsub channel.
/// This trait is used so that Sub and Pub can be generic over the channel.
pub trait PubSubBehavior<T> {
    /// Try to get a message from the queue with the given message id.
    ///
    /// If the message is not yet present and a context is given, then its waker is registered in the subsriber wakers.
    fn get_message_with_context(&self, next_message_id: &mut u64, cx: Option<&mut Context<'_>>) -> Poll<WaitResult<T>>;

    /// Get the amount of messages that are between the given the next_message_id and the most recent message.
    /// This is not necessarily the amount of messages a subscriber can still received as it may have lagged.
    fn available(&self, next_message_id: u64) -> u64;

    /// Try to publish a message to the queue.
    ///
    /// If the queue is full and a context is given, then its waker is registered in the publisher wakers.
    fn publish_with_context(&self, message: T, cx: Option<&mut Context<'_>>) -> Result<(), T>;

    /// Publish a message immediately
    fn publish_immediate(&self, message: T);

    /// The amount of messages that can still be published without having to wait or without having to lag the subscribers
    fn space(&self) -> usize;

    /// Let the channel know that a subscriber has dropped
    fn unregister_subscriber(&self, subscriber_next_message_id: u64);

    /// Let the channel know that a publisher has dropped
    fn unregister_publisher(&self);
}

/// The result of the subscriber wait procedure
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum WaitResult<T> {
    /// The subscriber did not receive all messages and lagged by the given amount of messages.
    /// (This is the amount of messages that were missed)
    Lagged(u64),
    /// A message was received
    Message(T),
}