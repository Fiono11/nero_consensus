use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use general::{Message, NUMBER_OF_TOTAL_NODES};
use node::Node;
use ::{NodeId, NUMBER_OF_BYZANTINE_NODES};


#[derive(Debug)]
pub(crate) struct Network {
    pub(crate) nodes: HashMap<u64, Arc<Mutex<Node>>>,
    pub(crate) receiver: Arc<Mutex<Receiver<(NodeId, Message)>>>,
}

impl Network {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = channel();
        let mut nodes = HashMap::new();
        for i in 0..NUMBER_OF_BYZANTINE_NODES {
            nodes.insert(i as u64, Arc::new(Mutex::new(Node::new(NodeId(i as u64), true, Arc::new(Mutex::new(sender.clone()))))));
        }
        for i in NUMBER_OF_BYZANTINE_NODES..NUMBER_OF_TOTAL_NODES {
            nodes.insert(i as u64, Arc::new(Mutex::new(Node::new(NodeId(i as u64), false, Arc::new(Mutex::new(sender.clone()))))));
        }
        Network {
            nodes,
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
                Message::SendVote(ref _msg, byzantine, id) => {
                    if !byzantine {
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
                    else {
                        nodes
                            .get_mut(&id)
                            .unwrap()
                            .lock()
                            .unwrap()
                            .handle_message(&msg);
                    }
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