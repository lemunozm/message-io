use crate::network::{self, NetworkController, NetworkProcessor, NetEvent};
use crate::events::{self, EventSender, EventThread};
use crate::util::thread::{OTHER_THREAD_ERR};

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

pub enum Event<'a, S> {
    Network(NetEvent<'a>),
    Signal(S),
}

pub struct NodeHandler<S> {
    network: Arc<NetworkController>,
    signal: EventSender<S>,
    running: Arc<AtomicBool>
}

impl<S> NodeHandler<S> {
    fn new(network: NetworkController, signal: EventSender<S>) -> Self {
        Self { network: Arc::new(network), signal, running: Arc::new(AtomicBool::new(true)), }
    }

    pub fn network(&self) -> &NetworkController {
        &self.network
    }

    pub fn signal(&self) -> &EventSender<S> {
        &self.signal
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl<S: Send + 'static> Clone for NodeHandler<S> {
    fn clone(&self) -> Self {
        Self { network: self.network.clone(), signal: self.signal.clone(), running: self.running.clone() }
    }
}

pub struct Node<S: Send + 'static> {
    handler: NodeHandler<S>,
    network_processor: NetworkProcessor,
    signal_receiver: EventThread<S>,
}

impl<S: Send + 'static> Node<S> {
    pub fn new(event_callback: impl FnMut(Event<S>, &NodeHandler<S>) + Send + 'static) -> Node<S> {
        let (network_controller, network_processor) = network::split();
        let (signal_sender, signal_receiver) = events::split();

        let node_handler = NodeHandler::new(network_controller, signal_sender);
        let mut node = Node {
            handler: node_handler.clone(),
            network_processor,
            signal_receiver,
        };

        let event_callback = Arc::new(Mutex::new(event_callback));

        let net_event_callback = event_callback.clone();
        let net_node_handler = node_handler.clone();

        node.network_processor.run(move |net_event, network_thread_handler| {
            net_event_callback.lock().expect(OTHER_THREAD_ERR)(Event::Network(net_event), &net_node_handler);
            if net_node_handler.is_running() {
                network_thread_handler.finalize();
            }
        });

        node.signal_receiver.run(move |signal, event_thread_handler| {
            event_callback.lock().expect(OTHER_THREAD_ERR)(Event::Signal(signal), &node_handler);
            if node_handler.is_running() {
                event_thread_handler.finalize();
            }
        });

        node
    }

    pub fn handler(&self) -> &NodeHandler<S> {
        &self.handler
    }

    pub fn wait(&mut self) {
        self.network_processor.wait();
        self.signal_receiver.wait();
    }
}

