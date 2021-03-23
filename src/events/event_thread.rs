use super::queue::{EventQueue};

use crate::thread::{RunnableThread};

use std::time::{Duration};

/// A thread that would be read events received in a [`EventQueue`]
pub struct EventThread<E: Send + 'static> {
    thread: RunnableThread<EventQueue<E>>,
}

impl<E: Send> EventThread<E> {
    const SAMPLING_TIMEOUT: u64 = 50; //ms

    /// Creates the thread, without run it.
    pub fn default() -> Self {
        Self { thread: RunnableThread::new("message_io::EventThread", EventQueue::new()) }
    }

    /// Run a thread giving a callback that would be called when a event be received.
    /// Run over an already running thread will panic.
    pub fn run(&self, callback: impl Fn(E) + Send + 'static) {
        let timeout = Duration::from_millis(Self::SAMPLING_TIMEOUT);
        self.thread.run(move |event_queue| {
            if let Some(event) = event_queue.receive_timeout(timeout) {
                callback(event);
            }
        });
    }

    /// Check if the thread is running.
    pub fn is_running(&self) -> bool {
        self.thread.is_running()
    }

    /// Stop the thread. A thread stopped can be run it again.
    /// Stop over an already stopped thread will panic.
    pub fn stop(&self) {
        self.thread.stop();
    }

    /// Wait the thread until it stops.
    pub fn join(&self) {
        self.thread.join();
    }
}
