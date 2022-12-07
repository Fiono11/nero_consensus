mod node;
mod vote;
mod election;
mod general;
mod network;

#[macro_use]
extern crate log;
extern crate byteorder;
extern crate rand;
extern crate ring;
extern crate env_logger;

use byteorder::{LittleEndian, WriteBytesExt};
use rand::{thread_rng, Rng};
use ring::digest;
use std::collections::{BTreeSet, HashMap};
use std::ops::Deref;
use std::ptr::hash;
use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex};
use std::{clone, thread};
use std::process::id;
use std::thread::sleep;
use std::time::Duration;
use env_logger::{Builder, Env};
use election::{Election, ElectionHash};
use general::{Hash, NUMBER_OF_BYZANTINE_NODES, NUMBER_OF_TOTAL_NODES, NUMBER_OF_TXS};
use network::Network;
use node::NodeId;
use vote::Vote;

fn main() {
    let env = Env::default();

    Builder::from_env(env)
        .format_level(false)
        .format_timestamp_nanos()
        .init();

    let mut net = Network::new();
    net.run();
    let mut hashes = BTreeSet::new();

    for _ in 0..NUMBER_OF_TXS {
        let election_hash= ElectionHash::random();
        let vote = Vote::random(NodeId(id as u64), election_hash.clone());
        hashes.insert(election_hash.clone());

        let id = thread_rng().gen_range(0, net.nodes.len()) as u64;
        //let mut node = net.nodes.get_mut(&id).unwrap().lock().unwrap();
        //node.send_vote(vote.clone());

        //thread::sleep_ms(500);

        //let mut votes = Vec::new();

        //for id in 0..NUMBER_OF_TOTAL_NODES {
            let vote = Vote::random(NodeId(id as u64), election_hash.clone());
            //votes.push(vote.clone());
            let mut election = Election::new(election_hash.clone());
            let mut node = net.nodes.get_mut(&(id as u64)).unwrap().lock().unwrap();
            //node.insert_vote(vote.clone());
            //election.state.insert(vote.round, rs);
            //node.elections.insert(election_hash.clone(), election.clone());
            node.send_vote(vote.clone());
        //}

        //for i in 0..NUMBER_OF_TOTAL_NODES {
            //let mut node = net.nodes.get_mut(&(i as u64)).unwrap().lock().unwrap();
            //node.send_vote(votes[i].clone());
        //}
    }

    loop {
        let mut finished = true;
        for i in 0..NUMBER_OF_TOTAL_NODES {
            if net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone().len() != hashes.len() {
                finished = false;
            }
        }
        let decided = net.nodes.get(&0).unwrap().lock().unwrap().decided.clone();
        if finished {
            for i in 1..NUMBER_OF_TOTAL_NODES {
                let other_decided = net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone();
                for hash in hashes.iter() {
                    assert_eq!(decided.get(&hash.clone()).unwrap(), other_decided.get(&hash.clone()).unwrap());
                }
            }
            info!("CONSENSUS ACHIEVED!!!");
            break;
        }
    }
}













