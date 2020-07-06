use crossbeam_channel::{self, Sender, Receiver};

use std::time::Duration;
use std::thread::{self, JoinHandle};
use std::collections::{HashMap};

pub enum Event<Message, Signal, Endpoint>
//where Message: Send, Signal: Send, Endpoint: Send
{
    Message(Message, Endpoint),
    AddedEndpoint(Endpoint),
    RemovedEndpoint(Endpoint),
    Signal(Signal),
}

pub struct EventQueue<E> {
    receiver: Receiver<E>,
    event_sender: EventSender<E>,
}

impl<E: Send + 'static> EventQueue<E> {
    pub fn new() -> EventQueue<E>
    {
        let (sender, receiver) = crossbeam_channel::unbounded();
        EventQueue {
            receiver,
            event_sender: EventSender::new(sender),
        }
    }

    pub fn sender(&self) -> &EventSender<E> {
        &self.event_sender
    }

    pub fn receive(&mut self) -> E {
        self.receiver.recv().unwrap()
    }

    pub fn receive_event_timeout(&mut self, timeout: Duration) -> Option<E> {
        self.receiver.recv_timeout(timeout).ok()
    }
}


pub struct EventSender<E> {
    sender: Sender<E>,
    timer_registry: HashMap<usize, JoinHandle<()>>,
    last_timer_id: usize,
}

impl<E> EventSender<E>
where E: Send + 'static {
    fn new(sender: Sender<E>) -> EventSender<E> {
        EventSender {
            sender,
            timer_registry: HashMap::new(),
            last_timer_id: 0,
        }
    }

    pub fn send(&mut self, event: E) {
        self.sender.send(event).unwrap();
    }

    pub fn send_with_timer(&mut self, event: E, duration: Duration) {
        let sender = self.sender.clone();
        let timer_id = self.last_timer_id;
        let timer_handle = thread::spawn(move || {
            thread::sleep(duration);
            sender.send(event).unwrap();
        });
        self.timer_registry.insert(timer_id, timer_handle);
        self.last_timer_id += 1;
    }

    pub fn stop_timers(&self) {
        todo!();
    }
}

impl<E> Drop for EventSender<E> {
    fn drop(&mut self) {
        for (_, timer) in self.timer_registry.drain() {
            timer.join().unwrap();
        }
    }
}

impl<E> Clone for EventSender<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            timer_registry: HashMap::new(),
            last_timer_id: 0,
        }
    }
}

