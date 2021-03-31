use super::queue::{EventSender, EventQueue};

use crate::util::thread::{RunnableThread, ThreadHandler};

use std::time::{Duration};

/// A thread that would be read events received in a [`EventQueue`]
pub struct EventThread<E: Send + 'static> {
    sender: EventSender<E>,
    thread: RunnableThread<EventQueue<E>>,
}

impl<E: Send> From<EventQueue<E>> for EventThread<E> {
    /// Creates the thread from an existing [`EventQueue`], without run it.
    fn from(event_queue: EventQueue<E>) -> Self {
        Self {
            sender: event_queue.sender().clone(),
            thread: RunnableThread::new("message_io::event-thread", event_queue),
        }
    }
}

impl<E: Send> Default for EventThread<E> {
    /// Creates the thread, without run it.
    fn default() -> Self {
        EventThread::from(EventQueue::default())
    }
}

impl<E: Send> EventThread<E> {
    const SAMPLING_TIMEOUT: u64 = 50; //ms

    /// As a shortcut, it returns the thread along with its associateded sender.
    pub fn split() -> (EventSender<E>, EventThread<E>) {
        let event_queue = EventQueue::default();
        let event_sender = event_queue.sender().clone();
        let event_thread = EventThread::from(event_queue);

        (event_sender, event_thread)
    }

    /// Return a sharable and clonable sender instance associated to the internal [`EventQueue`]
    pub fn sender(&self) -> &EventSender<E> {
        &self.sender
    }

    /// Run a thread giving a callback that would be called when a event be received.
    /// Run over an already running thread will panic.
    pub fn run(&mut self, mut callback: impl FnMut(E) + Send + 'static) {
        let timeout = Duration::from_millis(Self::SAMPLING_TIMEOUT);
        self.thread
            .spawn(move |event_queue| {
                if let Some(event) = event_queue.receive_timeout(timeout) {
                    callback(event);
                }
            })
            .unwrap();
    }

    /// Returns a sharable & clonable handler of this thread to check its state or finalize it
    pub fn handler(&self) -> &ThreadHandler {
        &self.thread.handler()
    }

    /// Wait the thread until it finalizes.
    /// It will wait until a call to [`ThreadHandler::finalize()`] was performed and
    /// the thread finish its last processing.
    pub fn join(&mut self) {
        self.thread.join();
    }

    /// Stops and consumes this thread to retrieve the [`EventQueue`].
    pub fn take_event_queue(self) -> EventQueue<E> {
        self.thread.handler().finalize();
        self.thread.take_state().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn basic_pipeline() {
        let mut thread = EventThread::<usize>::default();
        let sender = thread.sender().clone();

        let times = Arc::new(Mutex::new(0));
        let times_inside = times.clone();
        thread.run(move |event| {
            assert_eq!(42, event);
            *times_inside.lock().unwrap() += 1;
        });

        sender.send(42);
        sender.send(42);

        std::thread::sleep(Duration::from_millis(100));
        thread.handler().finalize();
        thread.join();
        assert_eq!(2, *times.lock().unwrap());
    }

    #[test]
    fn drop_while_finalizing() {
        let mut thread = EventThread::<usize>::default();
        thread.run(|_| ());
        thread.handler().finalize();
    }

    #[test]
    fn drop_while_running() {
        let mut thread = EventThread::<usize>::default();
        thread.run(|_| ());
    }
}
