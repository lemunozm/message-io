mod queue;
mod event_thread;

pub use queue::{EventQueue, EventSender};
pub use event_thread::{EventThread};

pub fn split<E: Send>() -> (EventSender<E>, EventThread<E>) {
    let mut event_queue = EventQueue::default();
    let event_sender = event_queue.sender().clone();
    let event_thread = EventThread::from(event_queue);

    (event_sender, event_thread)
}
