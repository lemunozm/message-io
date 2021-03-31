use crate::network::{self, NetworkController, NetworkProcessor, NetEvent};
use crate::events::{EventSender, EventThread};
use crate::util::thread::{OTHER_THREAD_ERR, ThreadHandler};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Event returned by the node when some network or signal is received.
pub enum NodeEvent<'a, S> {
    /// The event comes from the network.
    /// See [`NetEvent`] to know about the different network events.
    Net(NetEvent<'a>),

    /// The event comes from an user signal.
    /// See [`EventSender`] to know about how to send signals.
    Signal(S),
}

impl<'a, S> NodeEvent<'a, S> {
    pub fn net(self) -> NetEvent<'a> {
        match self {
            NodeEvent::Net(net_event) => net_event,
            NodeEvent::Signal(..) => panic!("NodeEvent must be a NetEvent"),
        }
    }

    pub fn signal(self) -> S {
        match self {
            NodeEvent::Net(..) => panic!("NodeEvent must be a NetEvent"),
            NodeEvent::Signal(signal) => signal,
        }
    }
}

/// A shareable and clonable entity that allows to deal with
/// the network, send signals and stop the node.
pub struct NodeHandler<S> {
    network: Arc<NetworkController>,
    signals: EventSender<S>,
    running: Arc<AtomicBool>, // Used as a cached value of thread handlers running.
    network_thread_handler: ThreadHandler,
    event_thread_handler: ThreadHandler,
}

impl<S> NodeHandler<S> {
    fn new(
        network: NetworkController,
        signals: EventSender<S>,
        network_thread_handler: ThreadHandler,
        event_thread_handler: ThreadHandler,
    ) -> Self {
        Self {
            network: Arc::new(network),
            signals,
            running: Arc::new(AtomicBool::new(false)),
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
    pub fn signals(&self) -> &EventSender<S> {
        &self.signals
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
            signals: self.signals.clone(),
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

impl<S: Send + 'static> Default for Node<S> {
    fn default() -> Node<S> {
        let (network_controller, mut network_processor) = network::split();
        let (signal_sender, signal_receiver) = EventThread::split();

        // In this way, the network methods used during node configuration
        // (before calling run()) will works since its internal poll events are processing.
        network_processor.run_to_cache();

        let node_handler = NodeHandler::new(
            network_controller,
            signal_sender,
            network_processor.handler().clone(),
            signal_receiver.handler().clone(),
        );

        Node { handler: node_handler, network_processor, signal_receiver }
    }
}

impl<S: Send + 'static> Node<S> {
    /// As a shortcut, returns the `Node` along with its `NodeHandler`.
    pub fn split() -> (NodeHandler<S>, Node<S>) {
        let node = Node::default();
        (node.handler().clone(), node)
    }

    /// Run asynchronously in order to events into the `event_callback`.
    /// After this call you could want to call [`Node::wait()`] to block the main thread.
    ///
    /// If other `Node::run()` is performed after this run() action,
    /// the previous will be stopped before.
    pub fn run(mut self, event_callback: impl FnMut(NodeEvent<S>) + Send + 'static) -> Node<S> {
        self.handler.stop();
        self.handler.running.store(true, Ordering::Relaxed);

        let event_callback = Arc::new(Mutex::new(event_callback));

        // User callback locked until the function ends, to avoid processing stops while
        // the node is configuring.
        let _locked = event_callback.lock().expect(OTHER_THREAD_ERR);

        let network_event_callback = event_callback.clone();
        let node_handler = self.handler().clone();
        self.network_processor.run(move |net_event| {
            let mut event_callback = network_event_callback.lock().expect(OTHER_THREAD_ERR);
            if node_handler.is_running() {
                event_callback(NodeEvent::Net(net_event));
            }
        });

        let signal_event_callback = event_callback.clone();
        let node_handler = self.handler().clone();
        self.signal_receiver.run(move |signal| {
            let mut event_callback = signal_event_callback.lock().expect(OTHER_THREAD_ERR);
            if node_handler.is_running() {
                event_callback(NodeEvent::Signal(signal));
            }
        });

        self
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
        self.signal_receiver.join();
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
        let mut node = Node::<()>::default();
        let handler = node.handler().clone();
        assert!(!handler.is_running());
        node = node.run(move |_| {
            std::thread::sleep(*STEP_DURATION);
            handler.stop();
            handler.stop(); // nothing happens
        });
        node.handler().signals().send(()); // Send a signal to processing the callback.
        assert!(node.handler().is_running());
        node.wait();
        assert!(!node.handler().is_running());
    }

    #[test]
    fn stop_from_outside() {
        let mut node = Node::<()>::default();
        assert!(!node.handler().is_running());

        node = node.run(|_| {
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
        let _node = Node::<()>::default().run(|_| ());
    }
}
