//! A queue for sending values between asynchronous tasks.
//!
//! Similar to a [`Channel`](crate::channel::Channel), however [`PriorityChannel`] sifts higher priority items to the front of the queue.
//! Priority is determined by the `Ord` trait. Priority behavior is determined by the [`Kind`](heapless::binary_heap::Kind) parameter of the channel.

use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub use heapless::binary_heap::{Kind, Max, Min};
use heapless::BinaryHeap;

use super::blocking_mutex::RawMutex;
use super::blocking_mutex::Mutex;
use super::channel::{DynamicChannel, DynamicReceiver, DynamicSender, TryReceiveError, TrySendError};
use super::waitqueue::WakerRegistration;

/// Send-only access to a [`PriorityChannel`].
pub struct Sender<'ch, M, T, K, const N: usize>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    channel: &'ch PriorityChannel<M, T, K, N>,
}

impl<'ch, M, T, K, const N: usize> Clone for Sender<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    fn clone(&self) -> Self {
        Sender { channel: self.channel }
    }
}

impl<'ch, M, T, K, const N: usize> Copy for Sender<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
}

impl<'ch, M, T, K, const N: usize> Sender<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    /// Sends a value.
    ///
    /// See [`PriorityChannel::send()`]
    pub fn send(&self, message: T) -> SendFuture<'ch, M, T, K, N> {
        self.channel.send(message)
    }

    /// Attempt to immediately send a message.
    ///
    /// See [`PriorityChannel::send()`]
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.channel.try_send(message)
    }

    /// Allows a poll_fn to poll until the channel is ready to send
    ///
    /// See [`PriorityChannel::poll_ready_to_send()`]
    pub fn poll_ready_to_send(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.channel.poll_ready_to_send(cx)
    }
}

impl<'ch, M, T, K, const N: usize> From<Sender<'ch, M, T, K, N>> for DynamicSender<'ch, T>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    fn from(s: Sender<'ch, M, T, K, N>) -> Self {
        Self { channel: s.channel }
    }
}

/// Receive-only access to a [`PriorityChannel`].
pub struct Receiver<'ch, M, T, K, const N: usize>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    channel: &'ch PriorityChannel<M, T, K, N>,
}

impl<'ch, M, T, K, const N: usize> Clone for Receiver<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    fn clone(&self) -> Self {
        Receiver { channel: self.channel }
    }
}

impl<'ch, M, T, K, const N: usize> Copy for Receiver<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
}

impl<'ch, M, T, K, const N: usize> Receiver<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    /// Receive the next value.
    ///
    /// See [`PriorityChannel::receive()`].
    pub fn receive(&self) -> ReceiveFuture<'_, M, T, K, N> {
        self.channel.receive()
    }

    /// Attempt to immediately receive the next value.
    ///
    /// See [`PriorityChannel::try_receive()`]
    pub fn try_receive(&self) -> Result<T, TryReceiveError> {
        self.channel.try_receive()
    }

    /// Allows a poll_fn to poll until the channel is ready to receive
    ///
    /// See [`PriorityChannel::poll_ready_to_receive()`]
    pub fn poll_ready_to_receive(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.channel.poll_ready_to_receive(cx)
    }

    /// Poll the channel for the next item
    ///
    /// See [`PriorityChannel::poll_receive()`]
    pub fn poll_receive(&self, cx: &mut Context<'_>) -> Poll<T> {
        self.channel.poll_receive(cx)
    }
}

impl<'ch, M, T, K, const N: usize> From<Receiver<'ch, M, T, K, N>> for DynamicReceiver<'ch, T>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    fn from(s: Receiver<'ch, M, T, K, N>) -> Self {
        Self { channel: s.channel }
    }
}

/// Future returned by [`PriorityChannel::receive`] and  [`Receiver::receive`].
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReceiveFuture<'ch, M, T, K, const N: usize>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    channel: &'ch PriorityChannel<M, T, K, N>,
}

impl<'ch, M, T, K, const N: usize> Future for ReceiveFuture<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        self.channel.poll_receive(cx)
    }
}

/// Future returned by [`PriorityChannel::send`] and  [`Sender::send`].
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendFuture<'ch, M, T, K, const N: usize>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    channel: &'ch PriorityChannel<M, T, K, N>,
    message: Option<T>,
}

impl<'ch, M, T, K, const N: usize> Future for SendFuture<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.message.take() {
            Some(m) => match self.channel.try_send_with_context(m, Some(cx)) {
                Ok(..) => Poll::Ready(()),
                Err(TrySendError::Full(m)) => {
                    self.message = Some(m);
                    Poll::Pending
                }
            },
            None => panic!("Message cannot be None"),
        }
    }
}

impl<'ch, M, T, K, const N: usize> Unpin for SendFuture<'ch, M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
}

struct ChannelState<T, K, const N: usize> {
    queue: BinaryHeap<T, K, N>,
    receiver_waker: WakerRegistration,
    senders_waker: WakerRegistration,
}

impl<T, K, const N: usize> ChannelState<T, K, N>
where
    T: Ord,
    K: Kind,
{
    const fn new() -> Self {
        ChannelState {
            queue: BinaryHeap::new(),
            receiver_waker: WakerRegistration::new(),
            senders_waker: WakerRegistration::new(),
        }
    }

    fn try_receive(&mut self) -> Result<T, TryReceiveError> {
        self.try_receive_with_context(None)
    }

    fn try_receive_with_context(&mut self, cx: Option<&mut Context<'_>>) -> Result<T, TryReceiveError> {
        if self.queue.len() == self.queue.capacity() {
            self.senders_waker.wake();
        }

        if let Some(message) = self.queue.pop() {
            Ok(message)
        } else {
            if let Some(cx) = cx {
                self.receiver_waker.register(cx.waker());
            }
            Err(TryReceiveError::Empty)
        }
    }

    fn poll_receive(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        if self.queue.len() == self.queue.capacity() {
            self.senders_waker.wake();
        }

        if let Some(message) = self.queue.pop() {
            Poll::Ready(message)
        } else {
            self.receiver_waker.register(cx.waker());
            Poll::Pending
        }
    }

    fn poll_ready_to_receive(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.receiver_waker.register(cx.waker());

        if !self.queue.is_empty() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }

    fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.try_send_with_context(message, None)
    }

    fn try_send_with_context(&mut self, message: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        match self.queue.push(message) {
            Ok(()) => {
                self.receiver_waker.wake();
                Ok(())
            }
            Err(message) => {
                if let Some(cx) = cx {
                    self.senders_waker.register(cx.waker());
                }
                Err(TrySendError::Full(message))
            }
        }
    }

    fn poll_ready_to_send(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.senders_waker.register(cx.waker());

        if !self.queue.len() == self.queue.capacity() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// A bounded channel for communicating between asynchronous tasks
/// with backpressure.
///
/// The channel will buffer up to the provided number of messages.  Once the
/// buffer is full, attempts to `send` new messages will wait until a message is
/// received from the channel.
///
/// Sent data may be reordered based on their priorty within the channel.
/// For example, in a [`Max`](heapless::binary_heap::Max) [`PriorityChannel`]
/// containing `u32`'s, data sent in the following order `[1, 2, 3]` will be received as `[3, 2, 1]`.
pub struct PriorityChannel<M, T, K, const N: usize>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    inner: Mutex<M, RefCell<ChannelState<T, K, N>>>,
}

impl<M, T, K, const N: usize> PriorityChannel<M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    /// Establish a new bounded channel. For example, to create one with a NoopMutex:
    ///
    /// ```
    /// use embassy_sync::priority_channel::{PriorityChannel, Max};
    /// use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    ///
    /// // Declare a bounded channel of 3 u32s.
    /// let mut channel = PriorityChannel::<NoopRawMutex, u32, Max, 3>::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(ChannelState::new())),
        }
    }

    fn lock<R>(&self, f: impl FnOnce(&mut ChannelState<T, K, N>) -> R) -> R {
        self.inner.lock(|rc| f(&mut *unwrap!(rc.try_borrow_mut())))
    }

    fn try_receive_with_context(&self, cx: Option<&mut Context<'_>>) -> Result<T, TryReceiveError> {
        self.lock(|c| c.try_receive_with_context(cx))
    }

    /// Poll the channel for the next message
    pub fn poll_receive(&self, cx: &mut Context<'_>) -> Poll<T> {
        self.lock(|c| c.poll_receive(cx))
    }

    fn try_send_with_context(&self, m: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        self.lock(|c| c.try_send_with_context(m, cx))
    }

    /// Allows a poll_fn to poll until the channel is ready to receive
    pub fn poll_ready_to_receive(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.lock(|c| c.poll_ready_to_receive(cx))
    }

    /// Allows a poll_fn to poll until the channel is ready to send
    pub fn poll_ready_to_send(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.lock(|c| c.poll_ready_to_send(cx))
    }

    /// Get a sender for this channel.
    pub fn sender(&self) -> Sender<'_, M, T, K, N> {
        Sender { channel: self }
    }

    /// Get a receiver for this channel.
    pub fn receiver(&self) -> Receiver<'_, M, T, K, N> {
        Receiver { channel: self }
    }

    /// Send a value, waiting until there is capacity.
    ///
    /// Sending completes when the value has been pushed to the channel's queue.
    /// This doesn't mean the value has been received yet.
    pub fn send(&self, message: T) -> SendFuture<'_, M, T, K, N> {
        SendFuture {
            channel: self,
            message: Some(message),
        }
    }

    /// Attempt to immediately send a message.
    ///
    /// This method differs from [`send`](PriorityChannel::send) by returning immediately if the channel's
    /// buffer is full, instead of waiting.
    ///
    /// # Errors
    ///
    /// If the channel capacity has been reached, i.e., the channel has `n`
    /// buffered values where `n` is the argument passed to [`PriorityChannel`], then an
    /// error is returned.
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.lock(|c| c.try_send(message))
    }

    /// Receive the next value.
    ///
    /// If there are no messages in the channel's buffer, this method will
    /// wait until a message is sent.
    pub fn receive(&self) -> ReceiveFuture<'_, M, T, K, N> {
        ReceiveFuture { channel: self }
    }

    /// Attempt to immediately receive a message.
    ///
    /// This method will either receive a message from the channel immediately or return an error
    /// if the channel is empty.
    pub fn try_receive(&self) -> Result<T, TryReceiveError> {
        self.lock(|c| c.try_receive())
    }
}

/// Implements the DynamicChannel to allow creating types that are unaware of the queue size with the
/// tradeoff cost of dynamic dispatch.
impl<M, T, K, const N: usize> DynamicChannel<T> for PriorityChannel<M, T, K, N>
where
    T: Ord,
    K: Kind,
    M: RawMutex,
{
    fn try_send_with_context(&self, m: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        PriorityChannel::try_send_with_context(self, m, cx)
    }

    fn try_receive_with_context(&self, cx: Option<&mut Context<'_>>) -> Result<T, TryReceiveError> {
        PriorityChannel::try_receive_with_context(self, cx)
    }

    fn poll_ready_to_send(&self, cx: &mut Context<'_>) -> Poll<()> {
        PriorityChannel::poll_ready_to_send(self, cx)
    }

    fn poll_ready_to_receive(&self, cx: &mut Context<'_>) -> Poll<()> {
        PriorityChannel::poll_ready_to_receive(self, cx)
    }

    fn poll_receive(&self, cx: &mut Context<'_>) -> Poll<T> {
        PriorityChannel::poll_receive(self, cx)
    }
}