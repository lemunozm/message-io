use crossbeam::channel::{self, Sender, Receiver, select};

use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, Duration};
use std::collections::{BTreeMap};

/// A generic and synchronized queue where the user can send and receive events.
/// See [`EventSender`] to see how send events.
/// This entity can be used as an utility for the [`crate::network`] module redirecting the
/// network events to process them later from here.
pub struct EventQueue<E> {
    associated_senders: Arc<AtomicUsize>,
    event_sender: EventSender<E>, // Should be before receiver in order to drop first.
    receiver: Receiver<E>,
    timer_receiver: Receiver<(Instant, E)>,
    priority_receiver: Receiver<E>,
    timers: BTreeMap<Instant, E>,
}

impl<E> EventQueue<E>
where E: Send + 'static
{
    /// Creates a new event queue for generic incoming events.
    pub fn new() -> EventQueue<E> {
        let (sender, receiver) = channel::unbounded();
        let (timer_sender, timer_receiver) = channel::unbounded();
        let (priority_sender, priority_receiver) = channel::unbounded();
        let associated = Arc::new(AtomicUsize::new(0));
        EventQueue {
            associated_senders: associated.clone(),
            event_sender: EventSender::new(associated, sender, timer_sender, priority_sender),
            receiver,
            timer_receiver,
            priority_receiver,
            timers: BTreeMap::new(),
        }
    }

    /// Returns the internal sender reference to this queue.
    /// This reference can be safety cloned and shared to other threads
    /// in order to get several senders to the same queue.
    pub fn sender(&mut self) -> &mut EventSender<E> {
        &mut self.event_sender
    }

    fn enque_timers(&mut self) {
        for timer in self.timer_receiver.try_iter() {
            self.timers.insert(timer.0, timer.1);
        }
    }

    /// Blocks the current thread until an event is received by this queue.
    pub fn receive(&mut self) -> E {
        self.enque_timers();
        // Since [`EventQueue`] always has a sender attribute,
        // any call to [`receive()`] always has a living sender in that time
        // and the channel never can be considered disconnected.
        if !self.priority_receiver.is_empty() {
            self.priority_receiver.recv().unwrap()
        }
        else if self.timers.is_empty() {
            select! {
                recv(self.receiver) -> event => event.unwrap(),
                recv(self.priority_receiver) -> event => event.unwrap(),
            }
        }
        else {
            let next_instant = *self.timers.iter().next().unwrap().0;
            if next_instant <= Instant::now() {
                self.timers.remove(&next_instant).unwrap()
            }
            else {
                select! {
                    recv(self.receiver) -> event => event.unwrap(),
                    recv(self.priority_receiver) -> event => event.unwrap(),
                    recv(channel::at(next_instant)) -> _ => {
                        self.timers.remove(&next_instant).unwrap()
                    }
                }
            }
        }
    }

    /// Blocks the current thread until an event is received by this queue or timeout is exceeded.
    /// If timeout is reached a None is returned, otherwise the event is returned.
    pub fn receive_timeout(&mut self, timeout: Duration) -> Option<E> {
        self.enque_timers();

        if !self.priority_receiver.is_empty() {
            Some(self.priority_receiver.recv().unwrap())
        }
        else if self.timers.is_empty() {
            select! {
                recv(self.receiver) -> event => Some(event.unwrap()),
                recv(self.priority_receiver) -> event => Some(event.unwrap()),
                default(timeout) => None
            }
        }
        else {
            let next_instant = *self.timers.iter().next().unwrap().0;
            if next_instant <= Instant::now() {
                self.timers.remove(&next_instant)
            }
            else {
                select! {
                    recv(self.receiver) -> event => Some(event.unwrap()),
                    recv(self.priority_receiver) -> event => Some(event.unwrap()),
                    recv(channel::at(next_instant)) -> _ => {
                        self.timers.remove(&next_instant)
                    }
                    default(timeout) => None
                }
            }
        }
    }
}

impl<E> Drop for EventQueue<E> {
    fn drop(&mut self) {
        if self.associated_senders.load(Ordering::Relaxed) > 1 {
            // > 1 because the own EventQueue has an undropped EventSender.
            panic!(
                "'EventQueue' dropped before its associated 'EventSender' instances. \
                Ensure that all 'EventSender' has been dropped first. \
                If you are storing the 'Network' and the 'EventQueue' in the same struct \
                ensure that the 'Network' is on top of the 'EventQueue' to be dropped first. \
                If your are using 'Network::split()' or 'Network::split_and_map()' functions, \
                you need to deestructure the returned tuple."
            );
        }
    }
}

impl<E> Default for EventQueue<E>
where E: Send + 'static
{
    fn default() -> Self {
        Self::new()
    }
}

/// Struct used to send events into a [`EventQueue`].
/// This type can only be generated by the receiver `EventQueue`.
pub struct EventSender<E> {
    associated_senders: Arc<AtomicUsize>,
    sender: Sender<E>,
    timer_sender: Sender<(Instant, E)>,
    priority_sender: Sender<E>,
}

impl<E> EventSender<E>
where E: Send + 'static
{
    const EVENT_SENDING_ERROR: &'static str =
        "The associated EventQueue has been removed before an EventSender.";

    fn new(
        associated_senders: Arc<AtomicUsize>,
        sender: Sender<E>,
        timer_sender: Sender<(Instant, E)>,
        priority_sender: Sender<E>,
    ) -> EventSender<E>
    {
        associated_senders.fetch_add(1, Ordering::SeqCst);
        EventSender { associated_senders, sender, timer_sender, priority_sender }
    }

    /// Send instantly an event to the event queue.
    pub fn send(&self, event: E) {
        self.sender.send(event).expect(Self::EVENT_SENDING_ERROR);
    }

    /// Send instantly an event that would be process before any other event sent
    /// by the [`crate::network::Network::send()] method.
    /// Successive calls to send_with_priority will maintain the order of arrival.
    pub fn send_with_priority(&self, event: E) {
        self.priority_sender.send(event).expect(Self::EVENT_SENDING_ERROR);
    }

    /// Send a timed event to the [`EventQueue`].
    /// The event only will be sent after the specific duration, never before.
    /// If the [`EventSender`] is dropped, the event will be generated as well.
    pub fn send_with_timer(&mut self, event: E, duration: Duration) {
        let when = Instant::now() + duration;
        self.timer_sender.send((when, event)).expect(Self::EVENT_SENDING_ERROR);
    }
}

impl<E> Drop for EventSender<E> {
    fn drop(&mut self) {
        self.associated_senders.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<E> Clone for EventSender<E>
where E: Send + 'static
{
    fn clone(&self) -> Self {
        EventSender::new(
            self.associated_senders.clone(),
            self.sender.clone(),
            self.timer_sender.clone(),
            self.priority_sender.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // This high delay is for ensure to works CI machines that offers really slow resources.
    // For a estandar execution, a value of 1ms is enough for the 99% of cases.
    const DELAY: u64 = 2000; //ms

    lazy_static::lazy_static! {
        static ref ZERO_MS: Duration = Duration::from_millis(0);
        static ref TIMER_TIME: Duration = Duration::from_millis(100);
        static ref TIMEOUT: Duration = *TIMER_TIME * 2  + Duration::from_millis(DELAY);
    }

    #[test]
    fn waiting_timer_event() {
        let mut queue = EventQueue::new();
        queue.sender().send_with_timer("Timed", *TIMER_TIME);
        assert_eq!(queue.receive_timeout(*TIMEOUT).unwrap(), "Timed");
    }

    #[test]
    fn standard_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send("first");
        queue.sender().send("second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "second");
    }

    #[test]
    fn priority_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send("standard");
        queue.sender().send_with_priority("priority_first");
        queue.sender().send_with_priority("priority_second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority_first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority_second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard");
    }

    #[test]
    fn timer_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send_with_timer("timed_last", *TIMER_TIME * 2);
        queue.sender().send_with_timer("timed_short", *TIMER_TIME);

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed_short");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed_last");
    }

    #[test]
    fn default_and_timer_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send("standard_first");
        queue.sender().send("standard_second");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard_first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard_second");
    }

    #[test]
    fn priority_and_timer_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send_with_priority("priority");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed");
    }

    #[test]
    #[should_panic]
    fn drop_queue_before_sender() {
        let mut queue = EventQueue::<()>::new();
        let sender = queue.sender().clone();
        drop(queue);
        drop(sender);
    }
}
