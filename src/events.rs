use crossbeam_channel::{self, Sender, Receiver, select};

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use std::thread::{self, JoinHandle};
use std::collections::{HashMap};

const TIMER_SAMPLING_CHECK: u64 = 50; //ms

pub struct EventQueue<E> {
    event_sender: EventSender<E>, // Should be before receiver in order to drop first.
    receiver: Receiver<E>,
    priority_receiver: Receiver<E>,
}

impl<E> EventQueue<E>
where E: Send + 'static
{
    /// Creates a new event queue for generic incoming events.
    pub fn new() -> EventQueue<E> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let (priority_sender, priority_receiver) = crossbeam_channel::unbounded();
        EventQueue {
            receiver,
            priority_receiver,
            event_sender: EventSender::new(sender, priority_sender),
        }
    }

    /// Returns the internal sender reference to this queue.
    /// This reference can be safety cloned and shared to other threads
    /// in order to make several senders to the same queue.
    pub fn sender(&mut self) -> &mut EventSender<E> {
        &mut self.event_sender
    }

    /// Blocks the current thread until an event is received by this queue.
    pub fn receive(&mut self) -> E {
        let event = if !self.priority_receiver.is_empty() {
            self.priority_receiver.recv()
        }
        else {
            select! {
                recv(self.receiver) -> event => event,
                recv(self.priority_receiver) -> event => event,
            }
        };

        match event {
            Ok(event) => event,
            // Since EventQueue always has a sender attribute,
            // any call to receive() always has a living sender in that time
            // and the channel never can be considered disconnected.
            Err(_) => unreachable!(),
        }
    }

    /// Blocks the current thread until an event is received by this queue or timeout is exceeded.
    /// If timeout is reached a None is returned, otherwise the event is returned.
    pub fn receive_timeout(&mut self, timeout: Duration) -> Option<E> {
        if !self.priority_receiver.is_empty() {
            Some(self.priority_receiver.recv().unwrap())
        }
        else {
            select! {
                recv(self.receiver) -> event => Some(event.unwrap()),
                recv(self.priority_receiver) -> event => Some(event.unwrap()),
                default(timeout) => None
            }
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

pub struct EventSender<E> {
    sender: Sender<E>,
    priority_sender: Sender<E>,
    timer_registry: HashMap<usize, JoinHandle<()>>,
    timers_running: Arc<AtomicBool>,
    last_timer_id: usize,
}

impl<E> EventSender<E>
where E: Send + 'static
{
    const EVENT_SENDING_ERROR: &'static str =
        "The associated EventQueue must be alive for sending an event";

    fn new(sender: Sender<E>, priority_sender: Sender<E>) -> EventSender<E> {
        EventSender {
            sender,
            priority_sender,
            timer_registry: HashMap::new(),
            timers_running: Arc::new(AtomicBool::new(true)),
            last_timer_id: 0,
        }
    }

    /// Send instantly an event to the event queue.
    pub fn send(&self, event: E) {
        self.sender.send(event).expect(Self::EVENT_SENDING_ERROR);
    }

    /// Send instantly an event that would be process before any other event sent by the send() method.
    /// Successive calls to send_with_priority will maintain the order of arrival.
    pub fn send_with_priority(&self, event: E) {
        self.priority_sender.send(event).expect(Self::EVENT_SENDING_ERROR);
    }

    /// Send a timed event to the [EventQueue].
    /// The event only will be sent after the specific duration, never before.
    /// If the [EventSender] is dropped, the event will not be sent.
    pub fn send_with_timer(&mut self, event: E, duration: Duration) {
        let sender = self.sender.clone();
        let timer_id = self.last_timer_id;
        let running = self.timers_running.clone();
        let mut time_acc = Duration::from_secs(0);
        let duration_step = Duration::from_millis(TIMER_SAMPLING_CHECK);
        let timer_handle = thread::Builder::new()
            .name("message-io: timer".into())
            .spawn(move || {
                while time_acc < duration {
                    thread::sleep(duration_step);
                    time_acc += duration_step;
                    if !running.load(Ordering::Relaxed) {
                        return
                    }
                }
                sender.send(event).expect(Self::EVENT_SENDING_ERROR);
            })
            .unwrap();
        self.timer_registry.insert(timer_id, timer_handle);
        self.last_timer_id += 1;
    }
}

impl<E> Drop for EventSender<E> {
    fn drop(&mut self) {
        self.timers_running.store(false, Ordering::Relaxed);
        for (_, timer) in self.timer_registry.drain() {
            timer.join().unwrap();
        }
    }
}

impl<E> Clone for EventSender<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            priority_sender: self.priority_sender.clone(),
            timer_registry: HashMap::new(),
            timers_running: Arc::new(AtomicBool::new(true)),
            last_timer_id: 0,
        }
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
        static ref TIMER_TIME: Duration = Duration::from_millis(TIMER_SAMPLING_CHECK);
        static ref TIMEOUT: Duration = Duration::from_millis(TIMER_SAMPLING_CHECK + DELAY);
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
        queue.sender().send_with_timer("timed", Duration::from_millis(TIMER_SAMPLING_CHECK));
        queue.sender().send("standard_first");
        queue.sender().send("standard_second");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard_first");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "standard_second");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed");
    }

    #[test]
    fn priority_and_time_events_order() {
        let mut queue = EventQueue::new();
        queue.sender().send_with_timer("timed", *TIMER_TIME);
        queue.sender().send_with_priority("priority");

        std::thread::sleep(*TIMEOUT);
        // The timed event has been received at this point

        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "priority");
        assert_eq!(queue.receive_timeout(*ZERO_MS).unwrap(), "timed");
    }
}
