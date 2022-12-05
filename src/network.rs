use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use general::{Message, NUMBER_OF_TOTAL_NODES};
use node::Node;
use NodeId;


#[derive(Debug)]
pub(crate) struct Network {
    pub(crate) nodes: HashMap<u64, Arc<Mutex<Node>>>,
    pub(crate) receiver: Arc<Mutex<Receiver<(NodeId, Message)>>>,
}

impl Network {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = channel();
        Network {
            nodes: (0..NUMBER_OF_TOTAL_NODES as u64)
                .map(|id| (id, Arc::new(Mutex::new(Node::new(NodeId(id), Arc::new(Mutex::new(sender.clone())))))))
                .collect(),
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub(crate) fn run(&self) {
        let receiver = self.receiver.clone();
        let mut nodes = self.nodes.clone();

        thread::spawn(move || loop {
            let (origin, msg) = receiver.lock().unwrap().recv().unwrap();
            let ids: Vec<u64> = nodes
                .iter()
                .map(|(id, _)| *id)
                .collect();
            match msg {
                Message::Vote(ref _msg) => {
                    ids
                        .iter()
                        .map(|id| {
                            nodes
                                .get_mut(&id)
                                .unwrap()
                                .lock()
                                .unwrap()
                                .handle_message(&msg)
                        })
                        .collect::<Vec<_>>();
                }
                Message::TimerExpired(ref _msg) => {
                    for id in ids {
                        if id == origin.0 {
                            nodes.get_mut(&id)
                                .unwrap()
                                .lock()
                                .unwrap()
                                .handle_message(&msg)
                        }
                    }
                }
                // Message that timer has expired
                _ => unreachable!(),
            }
        });
    }
}