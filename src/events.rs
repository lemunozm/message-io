use crossbeam_channel::{self, Sender, Receiver};

use std::time::Duration;
use std::thread::{self, JoinHandle};
use std::collections::{HashMap};


pub enum Event<Message, Signal, Endpoint> {
    Message(Message, Endpoint),
    AddedEndpoint(Endpoint),
    RemovedEndpoint(Endpoint),
    Signal(Signal),
    Start,
    Idle,
}

pub enum InputEvent<Message, Endpoint> {
    Message(Message, Endpoint),
    AddedEndpoint(Endpoint),
    RemovedEndpoint(Endpoint),
}

enum SelfEvent<Signal> {
    Signal(Signal),
    TimedSignal(Signal, usize),
}


pub fn new_event_system<M, S: Send + 'static, E>() -> (EventQueue<M, S, E>, InputEventHandle<M, E>) {
    let (input_sender, input_receiver) = crossbeam_channel::unbounded();

    (EventQueue::new(input_receiver), InputEventHandle::new(input_sender))
}


pub struct EventQueue<M, S, E> {
    self_sender: Sender<SelfEvent<S>>,
    self_receiver: Receiver<SelfEvent<S>>,
    input_receiver: Receiver<InputEvent<M, E>>,
    timer_registry: HashMap<usize, JoinHandle<()>>,
    last_timer_id: usize,
    is_idle: bool,
    started: bool
}

impl<M, S: Send + 'static, E> EventQueue<M, S, E>
{
    fn new(input_receiver: Receiver<InputEvent<M, E>>) -> EventQueue<M, S, E>
    {
        let (self_sender, self_receiver) = crossbeam_channel::unbounded();
        EventQueue {
            self_sender,
            self_receiver,
            input_receiver,
            timer_registry: HashMap::new(),
            last_timer_id: 0,
            is_idle: false,
            started: false,
        }
    }

    pub fn push_signal(&mut self, signal: S) {
        self.self_sender.send(SelfEvent::Signal(signal)).unwrap();
    }

    pub fn push_timed_signal(&mut self, signal: S, timeout: Duration) {
        let self_sender = self.self_sender.clone();
        let timer_id = self.last_timer_id;
        let timer_handle = thread::spawn(move || {
            thread::sleep(timeout);
            self_sender.send(SelfEvent::TimedSignal(signal, timer_id)).unwrap();
        });
        self.timer_registry.insert(timer_id, timer_handle);
        self.last_timer_id += 1;
    }

    pub fn pop_event(&mut self) -> Option<Event<M, S, E>> {
        self.pop_event_timeout(Duration::from_secs(0))
    }

    pub fn pop_event_timeout(&mut self, timeout: Duration) -> Option<Event<M, S, E>> {
        if self.is_just_start() {
            Some(Event::Start)
        }
        else if self.is_just_idle() {
            Some(Event::Idle)
        }
        else {
            crossbeam_channel::select! {
                recv(self.self_receiver) -> event => {
                    match event.unwrap() {
                        SelfEvent::Signal(signal) => Some(Event::Signal(signal)),
                        SelfEvent::TimedSignal(signal, id) => {
                            self.timer_registry.remove(&id);
                            Some(Event::Signal(signal))
                        }
                    }
                },
                recv(self.input_receiver) -> event => {
                    match event.unwrap() {
                        InputEvent::Message(message, endpoint) => Some(Event::Message(message, endpoint)),
                        InputEvent::AddedEndpoint(endpoint) => Some(Event::AddedEndpoint(endpoint)),
                        InputEvent::RemovedEndpoint(endpoint) => Some(Event::RemovedEndpoint(endpoint)),
                    }
                },
                default(timeout) => None
            }
        }
    }

    pub fn reset(&mut self) {
        self.started = false;
    }

    fn is_just_start(&mut self) -> bool {
        if !self.started { self.started = true; true } else { false }
    }

    fn is_just_idle(&mut self) -> bool {
        self.is_idle = self.self_receiver.is_empty() && self.input_receiver.is_empty() && !self.is_idle;
        self.is_idle
    }

}

impl<M, S, E> Drop for EventQueue<M, S, E> {
    fn drop(&mut self) {
        for (_, timer) in self.timer_registry.drain() {
            timer.join().unwrap();
        }
    }
}


pub struct InputEventHandle<M, E> {
    input_sender: Sender<InputEvent<M, E>>,
}

impl<M, E> InputEventHandle<M, E> {
    fn new(input_sender: Sender<InputEvent<M, E>>) -> InputEventHandle<M, E> {
        InputEventHandle { input_sender }
    }

    pub fn push(&mut self, event: InputEvent<M, E>) {
        self.input_sender.send(event).unwrap();
    }
}

impl<M, E> Clone for InputEventHandle<M, E> {
    fn clone(&self) -> Self {
        Self {
            input_sender: self.input_sender.clone(),
        }
    }
}
