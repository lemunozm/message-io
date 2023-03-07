use crossbeam_channel::{self, Sender, Receiver, select};

use std::time::{Instant, Duration};
use std::collections::{BTreeMap};

/// As a shortcut, it returns the sender and receiver queue as a tuple.
///
/// Equivalent to:
/// ```
/// struct MyEvent; // or usually an enum
///
/// use message_io::events::EventReceiver;
///
/// let event_queue = EventReceiver::<MyEvent>::default();
/// let event_sender = event_queue.sender().clone();
/// ```
pub fn split<E: Send + 'static>() -> (EventSender<E>, EventReceiver<E>) {
    let event_queue = EventReceiver::default();
    let event_sender = event_queue.sender().clone();

    (event_sender, event_queue)
}

/// An ID that represents a timer scheduled.
/// It can be used to cancel the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(Instant);

// Internal enum to enqueue different timer commands in a single queue
enum TimerCommand<E> {
    Create(E),
    Cancel,
}

/// A generic and synchronized queue where the user can send and receive events.
/// See [`EventSender`] to see how send events.
/// This entity can be used as an utility for the [`crate::network`] module redirecting the
/// network events to process them later from here.
pub struct EventReceiver<E> {
    event_sender: EventSender<E>, // Should be before receiver in order to drop first.
    receiver: Receiver<E>,
    timer_receiver: Receiver<(Instant, TimerCommand<E>)>,
    priority_receiver: Receiver<E>,
    timers: BTreeMap<Instant, E>,
}

impl<E> Default for EventReceiver<E>
where E: Send + 'static
{
    /// Creates a new event queue for generic incoming events.
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let (timer_sender, timer_receiver) = crossbeam_channel::unbounded();
        let (priority_sender, priority_receiver) = crossbeam_channel::unbounded();
        EventReceiver {
            event_sender: EventSender::new(sender, timer_sender, priority_sender),
            receiver,
            timer_receiver,
            priority_receiver,
            timers: BTreeMap::new(),
        }
    }
}

impl<E> EventReceiver<E>
where E: Send + 'static
{
    /// Returns the internal sender reference to this queue.
    /// This reference can be safety cloned and shared to other threads
    /// in order to get several senders to the same queue.
    pub fn sender(&self) -> &EventSender<E> {
        &self.event_sender
    }

    fn enque_timers(&mut self) {
        for timer in self.timer_receiver.try_iter() {
            match timer.1 {
                TimerCommand::Create(e) => self.timers.insert(timer.0, e),
                TimerCommand::Cancel => self.timers.remove(&timer.0),
            };
        }
    }

    /// Blocks the current thread until an event is received by this queue.
    pub fn receive(&mut self) -> E {
        self.enque_timers();
        // Since [`EventReceiver`] always has a sender attribute,
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
                    recv(crossbeam_channel::at(next_instant)) -> _ => {
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
                    recv(crossbeam_channel::at(next_instant)) -> _ => {
                        self.timers.remove(&next_instant)
                    }
                    default(timeout) => None
                }
            }
        }
    }

    /// Attempts to receive an event without blocking.
    /// Returns Some(E) if an event was received by this queue, otherwise returns None.
    pub fn try_receive(&mut self) -> Option<E> {
        self.enque_timers();

        if let Ok(priority_event) = self.priority_receiver.try_recv() {
            return Some(priority_event)
        }
        else if let Some(next_instant) = self.timers.iter().next() {
            if *next_instant.0 <= Instant::now() {
                let instant = *next_instant.0;
                return self.timers.remove(&instant)
            }
        }
        else if let Ok(event) = self.receiver.try_recv() {
            return Some(event)
        }

        None
    }
}

/// Struct used to send events into a [`EventReceiver`].
/// This type can only be generated by the receiver `EventReceiver`.
pub struct EventSender<E> {
    sender: Sender<E>,
    timer_sender: Sender<(Instant, TimerCommand<E>)>,
    priority_sender: Sender<E>,
}

impl<E> EventSender<E>
where E: Send + 'static
{
    fn new(
        sender: Sender<E>,
        timer_sender: Sender<(Instant, TimerCommand<E>)>,
        priority_sender: Sender<E>,
    ) -> EventSender<E> {
        EventSender { sender, timer_sender, priority_sender }
    }

    /// Send instantly an event to the event queue.
    pub fn send(&self, event: E) {
        self.sender.send(event).ok();
    }

    /// Send instantly an event that would be process before any other event sent
    /// by the [`EventSender::send()`] method.
    /// Successive calls to send_with_priority will maintain the order of arrival.
    pub fn send_with_priority(&self, event: E) {
        self.priority_sender.send(event).ok();
    }

    /// Send a timed event to the [`EventReceiver`].
    /// The event only will be sent after the specific duration, never before.
    /// If the [`EventSender`] is dropped, the event will be generated as well unless
    /// [`EventSender::cancel_timer()`] be called.
    pub fn send_with_timer(&self, event: E, duration: Duration) -> TimerId {
        let when = Instant::now() + duration;
        self.timer_sender.send((when, TimerCommand::Create(event))).ok();
        TimerId(when)
    }

    /// Remove a timer previously sent by [`EventSender::send_with_timer()`].
    /// The timer will not be receive by the [`EventReceiver`].
    pub fn cancel_timer(&self, timer_id: TimerId) {
        self.timer_sender.send((timer_id.0, TimerCommand::Cancel)).ok();
    }
}

impl<E> Clone for EventSender<E>
where E: Send + 'static
{
    fn clone(&self) -> Self {
        EventSender::new(
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
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("Timed", *TIMER_TIME);
        assert_eq!(queue.receive_timeout(*TIMEOUT).unwrap(), "Timed");
    }

    #[test]
    fn standard_events_order() {
        let mut queue = EventReceiver::default();
        queue.sender().send("first");
        queue.sender().send("second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "second");
    }

    #[test]
    fn priority_events_order() {
        let mut queue = EventReceiver::default();
        queue.sender().send("standard");
        queue.sender().send_with_priority("priority_first");
        queue.sender().send_with_priority("priority_second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority_first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority_second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard");
    }

    #[test]
    fn timer_events_order() {
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("timed_last", *TIMER_TIME * 2);
        queue.sender().send_with_timer("timed_short", *TIMER_TIME);

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed_short");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed_last");
    }

    #[test]
    fn default_and_timer_events_order() {
        let mut queue = EventReceiver::default();
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
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send_with_priority("priority");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed");
    }

    #[test]
    fn drop_queue_before_sender() {
        let queue = EventReceiver::<()>::default();
        let sender = queue.sender().clone();
        drop(queue);
        drop(sender);
    }

    #[test]
    fn standard_events_order_try_receive() {
        let mut queue = EventReceiver::default();
        queue.sender().send("first");
        queue.sender().send("second");
        assert_eq!(queue.try_receive().unwrap(), "first");
        assert_eq!(queue.try_receive().unwrap(), "second");
        assert_eq!(queue.try_receive(), None);
    }

    #[test]
    fn priority_events_order_try_receive() {
        let mut queue = EventReceiver::default();
        queue.sender().send("standard");
        queue.sender().send_with_priority("priority_first");
        queue.sender().send_with_priority("priority_second");
        assert_eq!(queue.try_receive().unwrap(), "priority_first");
        assert_eq!(queue.try_receive().unwrap(), "priority_second");
        assert_eq!(queue.try_receive().unwrap(), "standard");
        assert_eq!(queue.try_receive(), None);
    }

    #[test]
    fn timer_events_order_try_receive() {
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("timed_last", *TIMER_TIME * 2);
        queue.sender().send_with_timer("timed_short", *TIMER_TIME);

        assert_eq!(queue.try_receive(), None);
        std::thread::sleep(*TIMER_TIME);
        // The timed event has been received at this point
        assert_eq!(queue.try_receive().unwrap(), "timed_short");
        std::thread::sleep(*TIMER_TIME);
        assert_eq!(queue.try_receive().unwrap(), "timed_last");
        assert_eq!(queue.try_receive(), None);
    }

    #[test]
    fn default_and_timer_events_order_try_receive() {
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send("standard_first");
        queue.sender().send("standard_second");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.try_receive().unwrap(), "timed");
        assert_eq!(queue.try_receive().unwrap(), "standard_first");
        assert_eq!(queue.try_receive().unwrap(), "standard_second");
        assert_eq!(queue.try_receive(), None);
    }

    #[test]
    fn priority_and_timer_events_order_try_receive() {
        let mut queue = EventReceiver::default();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send_with_priority("priority");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.try_receive().unwrap(), "priority");
        assert_eq!(queue.try_receive().unwrap(), "timed");
        assert_eq!(queue.try_receive(), None);
    }

    #[test]
    fn cancel_timers() {
        let mut queue = EventReceiver::default();
        let id = queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().cancel_timer(id);

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point, but was cancelled.

        assert_eq!(queue.try_receive(), None);
    }
}
