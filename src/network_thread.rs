use crate::thread::{RunnableThread};

use crate::poll::{Poll, PollEvent};

use std::time::{Duration};

/// A thread that would be read poll events from the network resources.
pub struct NetworkThread<P: Send + 'static> {
    thread: RunnableThread<(Poll, P)>,
}

impl<P: Send> NetworkThread<P> {
    const SAMPLING_TIMEOUT: u64 = 50; //ms

    /// Creates the thread, without run it.
    /// The `processor` parameter is the processor instance that the user will use
    /// in order to process the network event.
    pub fn new(poll: Poll, processor: P) -> Self {
        Self { thread: RunnableThread::new("message_io::NetworkThread", (poll, processor)) }
    }

    /// Run a thread giving a callback that would be called when a a poll event be received.
    /// Run over an already running thread will panic.
    pub fn run(&self, callback: impl Fn((PollEvent, &mut P)) + Send + 'static) {
        let timeout = Some(Duration::from_millis(Self::SAMPLING_TIMEOUT));
        self.thread.run(move |(poll, processor)| {
            poll.process_event(timeout, |poll_event| {
                callback((poll_event, processor));
            });
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
