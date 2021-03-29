use crate::network::{self, NetworkController, NetworkProcessor, NetEvent};
use crate::events::{self, EventSender, EventThread};
use crate::util::thread::{OTHER_THREAD_ERR, ThreadHandler};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Event returned by the node when some network or signal is received.
pub enum NodeEvent<'a, S> {
    /// The event comes from the network.
    /// See [`NetEvent`] to know about the different network events.
    Network(NetEvent<'a>),

    /// The event comes from an user signal.
    /// See [`EventSender`] to know about how to send signals.
    Signal(S),
}

/// A shareable and clonable entity that allows to deal with
/// the network, send signals and stop the node.
pub struct NodeHandler<S> {
    network: Arc<NetworkController>,
    signal: EventSender<S>,
    running: Arc<AtomicBool>, // Used as a cached value of thread handlers running.
    network_thread_handler: ThreadHandler,
    event_thread_handler: ThreadHandler,
}

impl<S> NodeHandler<S> {
    fn new(
        network: NetworkController,
        signal: EventSender<S>,
        network_thread_handler: ThreadHandler,
        event_thread_handler: ThreadHandler,
        ) -> Self {
        Self {
            network: Arc::new(network),
            signal,
            running: Arc::new(AtomicBool::new(true)),
            network_thread_handler,
            event_thread_handler,
        }
    }

    /// Returns a reference to the NetworkController to deal with the network.
    /// See [`NetworkController`]
    pub fn network(&self) -> &NetworkController {
        &self.network
    }

    /// Returns a reference to the EventSender to send signals to the node.
    /// See [`EventSender`].
    pub fn signal(&self) -> &EventSender<S> {
        &self.signal
    }

    /// Finalizes the node processing.
    /// No more events would be processing after this call.
    /// After this call, if you have waiting by `Node::wait()` it would be unblocked.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.network_thread_handler.finalize();
        self.event_thread_handler.finalize();
    }

    /// Check if the node is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl<S: Send + 'static> Clone for NodeHandler<S> {
    fn clone(&self) -> Self {
        Self {
            network: self.network.clone(),
            signal: self.signal.clone(),
            running: self.running.clone(),
            network_thread_handler: self.network_thread_handler.clone(),
            event_thread_handler: self.event_thread_handler.clone(),
        }
    }
}

/// The main entity to manipulates the network and signal events easily.
/// The node run asynchornously
pub struct Node<S: Send + 'static> {
    handler: NodeHandler<S>,
    network_processor: NetworkProcessor,
    signal_receiver: EventThread<S>,
}

impl<S: Send + 'static> Node<S> {
    /// Creates a new node that works asynchronously and dispatching events into the `event_callback`.
    ///
    /// After this call and make your initial configuration,
    /// you could want to call [`Node::wait()`] to block the main thread.
    pub fn new(
        event_callback: impl FnMut(NodeEvent<S>, &NodeHandler<S>) + Send + 'static,
    ) -> Node<S> {
        let (network_controller, network_processor) = network::split();
        let (signal_sender, signal_receiver) = events::split();

        let node_handler = NodeHandler::new(
            network_controller,
            signal_sender,
            network_processor.handler().clone(),
            signal_receiver.handler().clone(),
        );

        let mut node = Node { handler: node_handler.clone(), network_processor, signal_receiver };

        let event_callback = Arc::new(Mutex::new(event_callback));

        let net_event_callback = event_callback.clone();
        let net_node_handler = node_handler.clone();

        node.network_processor.run(move |net_event| {
            let mut event_callback = net_event_callback.lock().expect(OTHER_THREAD_ERR);
            if net_node_handler.is_running() {
                event_callback(NodeEvent::Network(net_event), &net_node_handler);
            }
        });

        node.signal_receiver.run(move |signal| {
            let mut event_callback = event_callback.lock().expect(OTHER_THREAD_ERR);
            if node_handler.is_running() {
                event_callback(NodeEvent::Signal(signal), &node_handler);
            }
        });

        node
    }

    /// Return the handler of the node.
    /// The handler is a shareable and clonable entity that allows to deal with
    /// the network, send signals and stop the node.
    ///
    /// It is safe to call this function while the Node is blocked by [`Node::wait()`]
    pub fn handler(&self) -> &NodeHandler<S> {
        &self.handler
    }

    /// Blocks the current thread until the node be stopped.
    /// It will wait until a call to [`NodeHandler::stop()`] was performed
    /// and the last event processing finalizes.
    pub fn wait(&mut self) {
        self.network_processor.wait();
        self.signal_receiver.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};

    lazy_static::lazy_static! {
        static ref STEP_DURATION: Duration = Duration::from_millis(1000);
    }

    #[test]
    fn stop_from_inside() {
        let mut node = Node::<()>::new(|_, handler| {
            std::thread::sleep(*STEP_DURATION);
            handler.stop();
            handler.stop(); // nothing happens
        });
        node.handler().signal().send(()); // Send a signal to processing the callback.
        assert!(node.handler().is_running());
        node.wait();
        assert!(!node.handler().is_running());
    }

    #[test]
    fn stop_from_outside() {
        let mut node = Node::<()>::new(|_, _| {
            std::thread::sleep(*STEP_DURATION);
        });
        assert!(node.handler().is_running());
        node.handler().stop();
        node.handler().stop(); // nothing happens
        assert!(!node.handler().is_running());
        node.wait();
        assert!(!node.handler().is_running());
    }

    #[test]
    fn drop_while_running() {
        let _node = Node::<()>::new(|_, _| ());
    }
}
