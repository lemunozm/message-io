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
    fn from(mut event_queue: EventQueue<E>) -> Self {
        Self {
            sender: event_queue.sender().clone(),
            thread: RunnableThread::new("message_io::EventThread", event_queue),
        }
    }
}

impl<E: Send> Default for EventThread<E> {
    /// Creates the thread, without run it.
    fn default() -> Self {
        EventThread::from(EventQueue::new())
    }
}

impl<E: Send> EventThread<E> {
    const SAMPLING_TIMEOUT: u64 = 50; //ms

    /// Return a sharable and clonable sender instance associated to the internal [`EventQueue`]
    pub fn sender(&self) -> &EventSender<E> {
        &self.sender
    }

    /// Run a thread giving a callback that would be called when a event be received.
    /// Run over an already running thread will panic.
    pub fn run(&mut self, mut callback: impl FnMut(E, &ThreadHandler) + Send + 'static) {
        let timeout = Duration::from_millis(Self::SAMPLING_TIMEOUT);
        self.thread
            .spawn(move |event_queue, handler| {
                if let Some(event) = event_queue.receive_timeout(timeout) {
                    callback(event, handler);
                }
            })
            .unwrap();
    }

    /// Stop the thread.
    /// After the current processing event, no more events will be process.
    /// If you want to run the thread again, call [`EventThread::run()`].
    pub fn stop(&self) {
        self.thread.handler().finalize();
    }

    /// Check if the thread is running.
    pub fn is_running(&self) -> bool {
        self.thread.handler().is_running()
    }

    /// Wait the thread until it stops.
    /// It will wait until a call to [`EventThread::stop()`] was performed and
    /// the thread finish its last processing.
    pub fn wait(&mut self) {
        self.thread.join();
    }

    /// Stops and consumes this thread to retrieve the [`EventQueue`].
    pub fn take_event_queue(self) -> EventQueue<E> {
        self.thread.handler().finalize();
        self.thread.take_state().unwrap()
    }
}
