extern crate byteorder;
extern crate hex;
extern crate rand;
extern crate ring;
extern crate timer;
extern crate chrono;

use byteorder::{LittleEndian, WriteBytesExt};
use rand::{thread_rng, Rng};
use ring::digest;
use std::collections::{BTreeSet, HashMap};
use std::ops::Deref;
use std::ptr::hash;
use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex};
use std::thread;

const NUMBER_OF_BYZANTINE_NODES: usize = 1;
const NUMBER_OF_TOTAL_NODES: usize = 3 * NUMBER_OF_BYZANTINE_NODES + 1;
const QUORUM: usize = 2 * NUMBER_OF_BYZANTINE_NODES + 1;
const SEMI_QUORUM: usize = NUMBER_OF_BYZANTINE_NODES + 1;
const NUMBER_OF_TXS: usize = 10;
const TIMEOUT: usize = 1;

fn main() {
    let mut net = Network::new();
    net.run();
    let mut hashes = BTreeSet::new();

    for _ in 0..NUMBER_OF_TXS {
        let tx = Transaction::random();
        println!("sending new transaction into the network {}", &tx.hash());

        let id = thread_rng().gen_range(0, net.nodes.len()) as u64;
        let node = net.nodes.get_mut(&id).unwrap();
        node.lock()
            .unwrap()
            .handle_message(0, &Message::Transaction(tx.clone()));
        hashes.insert(tx.hash());

        thread::sleep_ms(500);
    }

    loop {
        let decided = net.nodes.get(&0).unwrap().lock().unwrap().decided.clone();
        if decided.len() == hashes.len() {
            for i in 1..NUMBER_OF_TOTAL_NODES {
                let other_decided = net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone();
                for hash in hashes.iter() {
                    assert_eq!(decided.get(&hash).unwrap(), other_decided.get(&hash).unwrap());
                }
            }
        }
    }
}

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash)]
struct Hash(Vec<u8>);

impl Hash {
    fn to_string(&self) -> String {
        hex::encode(&self.0)
    }
}

impl ::std::fmt::Display for Hash {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl ::std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

#[derive(Debug, Clone)]
enum Message {
    Vote(Vote),
    Transaction(Transaction),
    TimerExpired(Vote),
}

#[derive(Debug, Clone)]
struct Vote {
    hash: Hash,
    round: u32,
    value: u64,
    vote_type: VoteType,
    proof: Option<Vec<Hash>>,
}

impl Vote {
    fn new(hash: Hash, round: u32, value: u64, vote_type: VoteType, proof: Option<Vec<Hash>>) -> Self {
        Self { hash, round, value, vote_type, proof }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec![];
        buf.write_u32::<LittleEndian>(self.round).unwrap();
        buf.write_u64::<LittleEndian>(self.value).unwrap();
        buf
    }

    fn hash(&self) -> Hash {
        let digest = digest::digest(&digest::SHA256, &self.serialize());
        Hash(digest.as_ref().to_vec())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum VoteType {
    Vote,
    Commit,
}

#[derive(Debug, Clone)]
struct Transaction {
    nonce: u64,
    data: i32,
}

impl Transaction {
    fn random() -> Self {
        let mut rng = thread_rng();
        Transaction {
            nonce: rand::random::<u64>(),
            data: rng.gen_range(0, 10),
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec![];
        buf.write_i32::<LittleEndian>(self.data).unwrap();
        buf
    }

    fn hash(&self) -> Hash {
        let digest = digest::digest(&digest::SHA256, &self.serialize());
        Hash(digest.as_ref().to_vec())
    }
}

#[derive(Debug)]
struct Network {
    nodes: HashMap<u64, Arc<Mutex<Node>>>,
    receiver: Arc<Mutex<Receiver<(u64, Message)>>>,
}

impl Network {
    fn new() -> Self {
        let (sender, receiver) = channel();
        Network {
            nodes: (0..NUMBER_OF_TOTAL_NODES as u64)
                .map(|id| (id, Arc::new(Mutex::new(Node::new(id as u64, Arc::new(Mutex::new(sender.clone())))))))
                .collect(),
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    fn run(&self) {
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
                                .handle_message(origin, &msg)
                        })
                        .collect::<Vec<_>>();
                }
                Message::TimerExpired(ref _msg) => {
                    for id in ids {
                        if id == origin {
                            nodes.get_mut(&id)
                                .unwrap()
                                .lock()
                                .unwrap()
                                .handle_message(origin, &msg)
                        }
                    }
                }
                // Message that timer has expired
                _ => unreachable!(),
            }
        });
    }
}

#[derive(Debug, Clone)]
struct Election {
    hash: Hash,
    votes: HashMap<u32, Vec<Vote>>,
    vote_by_hash: HashMap<Hash, Vote>,
    is_decided: bool,
    vote_tally: HashMap<u64, u64>,
    commit_tally: HashMap<u64, u64>,
    current_value: u64,
    timed_out: bool,
    unvalidated_votes: HashMap<Vote, Vec<Vote>>,
}

impl Election {
    fn new(hash: Hash) -> Self {
        Election {
            hash,
            is_decided: false,
            vote_tally: HashMap::new(),
            commit_tally: HashMap::new(),
            votes: HashMap::new(),
            current_value: 0,
            vote_by_hash: HashMap::new(),
            unvalidated_votes: HashMap::new(),
            timed_out: false,
        }
    }

    fn insert_vote(&mut self, vote: Vote, own_vote: bool) {
        let mut votes: Vec<Vote> = Vec::new();
        match self.votes.get(&vote.round) {
            Some(v) => {
                votes = v.to_vec();
                votes.push(vote.clone())
            }
            None => {
                votes.push(vote.clone())
            }
        }
        self.votes.insert(vote.round, votes);
        self.tally_votes(&vote.round);
        if own_vote {
            self.current_value = vote.value;
        }
    }

    fn tally_votes(&mut self, round: &u32) {
        let votes = self.votes.get(round).unwrap();
        for vote in votes.iter() {
            if vote.vote_type == VoteType::Vote {
                match self.vote_tally.get(&vote.value) {
                    Some(count) => self.vote_tally.insert(vote.value, count + 1),
                    None => self.vote_tally.insert(vote.value, 1)
                };
            }
            else {
                match self.commit_tally.get(&vote.value) {
                    Some(count) => self.vote_tally.insert(vote.value, count + 1),
                    None => self.vote_tally.insert(vote.value, 1)
                };
            }
        }
    }

    fn validate_vote(&self, vote: Vote) -> bool {
        if vote.round != 0 {
            for hash in vote.proof.unwrap().iter() {
                if !self.vote_by_hash.contains_key(&hash) {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
struct Node {
    mempool: HashMap<Hash, Election>,
    id: u64,
    sender: Arc<Mutex<Sender<(u64, Message)>>>,
    decided: HashMap<Hash, u64>,
}

impl Node {
    fn new(id: u64, sender: Arc<Mutex<Sender<(u64, Message)>>>) -> Self {
        Node {
            id,
            sender,
            mempool: HashMap::new(),
            decided: HashMap::new(),
        }
    }

    fn handle_message(&mut self, origin: u64, msg: &Message) {
        match msg {
            Message::Vote(vote) => self.handle_vote(origin, vote),
            Message::Transaction(tx) => self.handle_transaction(tx),
            Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    fn handle_vote(&mut self, origin: u64, vote: &Vote) {
        if self.decided.contains_key(&vote.hash) {
            return;
        }
        if !self.mempool.contains_key(&vote.hash) {
            let mut election = Election::new(vote.hash.clone());
            election.insert_vote(vote.clone(), false);
            let mut own_vote = vote.clone();
            own_vote.round = 0;
            own_vote.vote_type = VoteType::Vote;
            self.send_query(own_vote.clone());
            election.insert_vote(own_vote.clone(), true);
        }
        else {
            let mut election = self.mempool.get(&vote.hash).unwrap().clone();
            let votes = election.votes.get(&vote.round).unwrap();
            if votes.len() > QUORUM && election.timed_out {
                self.decide_vote(election, vote.value, vote.hash.clone(), vote.round);
            }
        }
    }

    fn decide_vote(&mut self, mut election: Election, value: u64, hash: Hash, round: u32) {
        let mut highest_value = 0;
        let mut bool = false;
        for v in election.vote_tally.values() {
            if v > &(SEMI_QUORUM as u64) {
                bool = true;
            }
            if v > &highest_value {
                highest_value = *v;
            }
        }
        let proof: Option<Vec<Hash>> = Some(election.votes.get(&round).unwrap().iter().map(|vote| vote.hash()).collect());
        let mut next_round_vote = Vote::new(hash.clone(), round + 1, value, VoteType::Commit, proof);
        if election.vote_tally.get(&value).unwrap() > &(QUORUM as u64) {

        }
        else if election.commit_tally.get(&value).unwrap() > &(QUORUM as u64)  {
            election.is_decided = true;
            self.decided.insert(hash.clone(), value);
            return;
        }
        else if election.commit_tally.get(&0).unwrap() > &(SEMI_QUORUM as u64)  {
            next_round_vote.value = 0;
            next_round_vote.vote_type = VoteType::Vote;
        }
        else if bool {
            next_round_vote.value = election.current_value;
            next_round_vote.vote_type = VoteType::Vote;
        }
        else if !bool {
            next_round_vote.value = 0;
            next_round_vote.vote_type = VoteType::Vote;
        }
        else {
            next_round_vote.value = highest_value;
            next_round_vote.vote_type = VoteType::Vote;
        }
        election.insert_vote(next_round_vote, true);
        election.timed_out = false;
        self.mempool.insert(hash.clone(), election);
    }

    fn handle_timeout(&mut self, vote: &Vote) {
        let mut election = self.mempool.get(&vote.hash).unwrap().clone();
        let votes = election.votes.get(&vote.round).unwrap();
        if votes.len() > QUORUM && election.timed_out {
            self.decide_vote(election, vote.value, vote.hash.clone(), vote.round);
        }
    }

    fn handle_transaction(&mut self, tx: &Transaction) {
        if !self.mempool.contains_key(&tx.hash()) {
            let vote = Vote::new(tx.clone().hash(),0,tx.clone().nonce,VoteType::Vote, None);
            self.send_query(vote);
        }
    }

    fn send_query(&self, vote: Vote) {
        let msg = Message::Vote(Vote {
            hash: vote.hash,
            round: vote.round,
            value: vote.value,
            vote_type: vote.vote_type,
            proof: vote.proof,
        });
        self.sender.lock().unwrap().send((self.id, msg));
    }
}

fn set_timeout(node: Node, vote: Vote) {
    let timer = timer::Timer::new();
    let msg = Message::TimerExpired(vote.clone());
    let sender = node.sender.clone();
    timer.schedule_with_delay(chrono::Duration::seconds(TIMEOUT as i64), move || {
        sender.lock().unwrap().send((node.id, msg.clone()));
    });
}
