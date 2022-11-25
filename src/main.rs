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
use std::thread::sleep;
use std::time::Duration;

const NUMBER_OF_BYZANTINE_NODES: usize = 1;
const NUMBER_OF_TOTAL_NODES: usize = 3 * NUMBER_OF_BYZANTINE_NODES + 1;
const QUORUM: usize = 2 * NUMBER_OF_BYZANTINE_NODES + 1;
const SEMI_QUORUM: usize = NUMBER_OF_BYZANTINE_NODES + 1;
const NUMBER_OF_TXS: usize = 1;
const TIMEOUT: usize = 1;

fn main() {
    let mut net = Network::new();
    net.run();
    let mut hashes = BTreeSet::new();

    for _ in 0..NUMBER_OF_TXS {
        let tx = Transaction::random();
        println!("sending new transaction into the network {:#?}", &tx.hash());

        let id = thread_rng().gen_range(0, net.nodes.len()) as u64;
        let node = net.nodes.get_mut(&id).unwrap();
        node.lock()
            .unwrap()
            .handle_message(&Message::Transaction(tx.clone()));
        hashes.insert(tx.hash());

        //thread::sleep_ms(500);
    }

    loop {
        let decided = net.nodes.get(&0).unwrap().lock().unwrap().decided.clone();
        if decided.len() == hashes.len() {
            for i in 1..NUMBER_OF_TOTAL_NODES {
                let other_decided = net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone();
                for hash in hashes.iter() {
                    assert_eq!(decided.get(&hash.clone()).unwrap(), other_decided.get(&hash.clone()).unwrap());
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

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
enum Message {
    Vote(Vote),
    Transaction(Transaction),
    TimerExpired(Vote),
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy)]
struct NodeId(u64);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
struct Round(u32);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
struct Value(u64);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Vote {
    signer: NodeId,
    vote_hash: VoteHash,
    round: Round,
    value: Value,
    vote_type: VoteType,
    proof: Option<BTreeSet<VoteHash>>,
    election_hash: ElectionHash,
}

fn vote_hash(round: Round, value: Value) -> VoteHash {
    let mut buf = vec![];
    buf.write_u32::<LittleEndian>(round.0).unwrap();
    buf.write_u64::<LittleEndian>(value.0).unwrap();
    let digest = digest::digest(&digest::SHA256, &buf);
    VoteHash(Hash(digest.as_ref().to_vec()))
}

impl Vote {
    fn new(signer: NodeId, vote_hash: VoteHash, round: Round, value: Value, vote_type: VoteType, proof: Option<BTreeSet<VoteHash>>, election_hash: ElectionHash) -> Self {
        Self { signer, vote_hash, round, value, vote_type, proof, election_hash }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec![];
        buf.write_u32::<LittleEndian>(self.round.0).unwrap();
        buf.write_u64::<LittleEndian>(self.value.0).unwrap();
        buf
    }

    fn hash(&self) -> VoteHash {
        let digest = digest::digest(&digest::SHA256, &self.serialize());
        VoteHash(Hash(digest.as_ref().to_vec()))
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
enum VoteType {
    Vote,
    Commit,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Transaction {
    nonce: Value,
    data: i32,
}

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct ElectionHash(Hash);

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct VoteHash(Hash);

impl Transaction {
    fn random() -> Self {
        let mut rng = thread_rng();
        Transaction {
            nonce: Value(rand::random::<u64>()),
            data: rng.gen_range(0, 10),
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec![];
        buf.write_i32::<LittleEndian>(self.data).unwrap();
        buf
    }

    fn hash(&self) -> ElectionHash {
        let digest = digest::digest(&digest::SHA256, &self.serialize());
        ElectionHash(Hash(digest.as_ref().to_vec()))
    }
}

#[derive(Debug)]
struct Network {
    nodes: HashMap<u64, Arc<Mutex<Node>>>,
    receiver: Arc<Mutex<Receiver<(NodeId, Message)>>>,
}

impl Network {
    fn new() -> Self {
        let (sender, receiver) = channel();
        Network {
            nodes: (0..NUMBER_OF_TOTAL_NODES as u64)
                .map(|id| (id, Arc::new(Mutex::new(Node::new(NodeId(id), Arc::new(Mutex::new(sender.clone())))))))
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

#[derive(Debug, Clone)]
struct Election {
    hash: ElectionHash,
    votes: HashMap<Round, BTreeSet<Vote>>,
    vote_by_hash: HashMap<VoteHash, Vote>,
    is_decided: bool,
    vote_tally: HashMap<Value, u64>,
    commit_tally: HashMap<Value, u64>,
    current_value: Value,
    timed_out: bool,
    unvalidated_votes: HashMap<Vote, BTreeSet<Vote>>,
    voted: BTreeSet<Round>,
}

impl Election {
    fn new(hash: ElectionHash) -> Self {
        Election {
            hash,
            is_decided: false,
            vote_tally: HashMap::new(),
            commit_tally: HashMap::new(),
            votes: HashMap::new(),
            current_value: Value(0),
            vote_by_hash: HashMap::new(),
            unvalidated_votes: HashMap::new(),
            timed_out: false,
            voted: BTreeSet::new(),
        }
    }

    fn insert_vote(&mut self, vote: Vote, own_vote: bool) {
        let mut votes: BTreeSet<Vote> = BTreeSet::new();
        match self.votes.get(&vote.round) {
            Some(v) => {
                votes.insert(vote.clone());
            }
            None => {
                votes.insert(vote.clone());
            }
        }
        self.votes.insert(vote.round, votes);
        self.tally_votes(&vote.round);
        if own_vote {
            self.current_value = vote.value;
            self.voted.insert(vote.round);
        }
    }

    fn tally_votes(&mut self, round: &Round) {
        let votes = self.votes.get(round).unwrap();
        for vote in votes.iter() {
            if vote.vote_type == VoteType::Vote {
                match self.vote_tally.get(&vote.value) {
                    Some(count) => self.vote_tally.insert(vote.value, count + 1),
                    None => self.vote_tally.insert(vote.value, 1),
                };
            }
            else {
                match self.commit_tally.get(&vote.value) {
                    Some(count) => self.commit_tally.insert(vote.value, count + 1),
                    None => self.commit_tally.insert(vote.value, 1),
                };
            }
        }
    }

    fn validate_vote(&mut self, vote: Vote) {
        let mut valid = true;
        if vote.round != Round(0) {
            for hash in vote.clone().proof.unwrap().iter() {
                if !self.vote_by_hash.contains_key(&hash) {
                    valid = false;
                }
            }
        }
        if valid {
            self.insert_vote(vote.clone(), false);
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    elections: HashMap<ElectionHash, Election>,
    id: NodeId,
    sender: Arc<Mutex<Sender<(NodeId, Message)>>>,
    decided: HashMap<ElectionHash, Value>,
}

impl Node {
    fn new(id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>) -> Self {
        Node {
            id,
            sender,
            elections: HashMap::new(),
            decided: HashMap::new(),
        }
    }

    fn handle_message(&mut self, msg: &Message) {
        match msg {
            Message::Vote(vote) => self.handle_vote(vote),
            Message::Transaction(tx) => self.handle_transaction(tx),
            Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    fn handle_vote(&mut self, vote: &Vote) {
        println!("Node {:?} received from node {:?} vote {:#?}", self.id, vote.signer, vote);
        if self.decided.contains_key(&vote.election_hash) || vote.signer == self.id {
            return;
        }
        let mut election = Election::new(vote.election_hash.clone());
        match self.elections.get(&vote.election_hash) {
            Some(e) => election = e.clone(),
            None => ()//set_timeout(self.clone(), vote.clone()),
        }
        election.validate_vote(vote.clone());
        if let None = election.voted.get(&vote.round) {
            let mut own_vote = vote.clone();
            own_vote.round = Round(0);
            own_vote.vote_type = VoteType::Vote;
            election.insert_vote(own_vote.clone(), true);
            self.send_vote(own_vote.clone(), election.clone());
        }
        self.elections.insert(election.clone().hash, election.clone());
        let binding = election.clone();
        let votes = binding.votes.get(&vote.round).unwrap();
        if votes.len() > QUORUM && election.clone().timed_out {
            self.decide_vote(election.clone(), vote.value, vote.round);
        }
    }

    fn decide_vote(&mut self, mut election: Election, value: Value, round: Round) {
        let mut highest_value = Value(0);
        let mut bool = false;
        for v in election.vote_tally.values() {
            if v > &(SEMI_QUORUM as u64) {
                bool = true;
            }
            if v > &(highest_value.0 as u64) {
                highest_value = Value(*v);
            }
        }
        let proof: Option<BTreeSet<VoteHash>> = Some(election.votes.get(&round).unwrap().iter().map(|vote| vote.hash()).collect());
        let mut next_round_vote = Vote::new(self.id, vote_hash(Round(round.0 + 1), value), Round(round.0 + 1), value, VoteType::Commit, proof, election.hash.clone());
        if election.vote_tally.get(&value).unwrap() > &(QUORUM as u64) {

        }
        else if election.commit_tally.get(&value).unwrap() > &(QUORUM as u64)  {
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), value);
            return;
        }
        else if election.commit_tally.get(&Value(0)).unwrap() > &(SEMI_QUORUM as u64)  {
            next_round_vote.value = Value(0);
            next_round_vote.vote_type = VoteType::Vote;
        }
        else if bool {
            next_round_vote.value = election.current_value;
            next_round_vote.vote_type = VoteType::Vote;
        }
        else if !bool {
            next_round_vote.value = Value(0);
            next_round_vote.vote_type = VoteType::Vote;
        }
        else {
            next_round_vote.value = highest_value;
            next_round_vote.vote_type = VoteType::Vote;
        }
        election.insert_vote(next_round_vote.clone(), true);
        election.timed_out = false;
        election.voted.insert(next_round_vote.round);
        self.elections.insert(election.hash.clone(), election.clone());
        self.send_vote(next_round_vote.clone(), election.clone());
    }

    // fn first_vote

    fn handle_timeout(&mut self, vote: &Vote) {
        if !self.elections.contains_key(&vote.election_hash.clone()) {
            let vote = Vote::new(self.id.clone(), vote_hash(Round(0), vote.value),Round(0), vote.value,VoteType::Vote, None, vote.election_hash.clone());
            let mut election = Election::new(vote.election_hash.clone());
            election.insert_vote(vote.clone(), true);
            self.elections.insert(vote.election_hash.clone(), election.clone());
            self.send_vote(vote.clone(), election);
            set_timeout(self.clone(), vote.clone());
        }
        else {
            let mut election = self.elections.get(&vote.election_hash).unwrap().clone();
            let votes = election.votes.get(&vote.round).unwrap();
            if votes.len() > QUORUM && election.timed_out {
                self.decide_vote(election, vote.value, vote.round);
            }
        }
        println!("Vote: {:?}", vote);
    }

    fn handle_transaction(&mut self, tx: &Transaction) {
        if !self.elections.contains_key(&tx.hash()) {
            let vote = Vote::new(self.id, vote_hash(Round(0), tx.nonce),Round(0),tx.nonce,VoteType::Vote, None, tx.clone().hash());
            let mut election = Election::new(vote.election_hash.clone());
            self.elections.insert(tx.hash(), election.clone());
            election.insert_vote(vote.clone(), true);
            self.send_vote(vote.clone(), election);
            //set_timeout(self.clone(), vote.clone());
        }
    }

    fn send_vote(&self, vote: Vote, election: Election) {
        let msg = Message::Vote(Vote {
            signer: self.id,
            vote_hash: vote.vote_hash,
            round: vote.round,
            value: vote.value,
            vote_type: vote.vote_type,
            proof: vote.proof,
            election_hash: election.hash.clone(),
        });
        self.sender.lock().unwrap().send((self.id, msg));
        println!("Node {:?} voted value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, election.hash.clone());
        println!("State of election {:?}: {:#?}", election.hash, election);
    }
}

fn set_timeout(node: Node, vote: Vote) {
    let msg = Message::TimerExpired(vote.clone());
    let sender = node.sender.clone();
    thread::spawn(move || {
        sleep(Duration::from_secs(3));
        println!("Setting timeout...");
        sender.lock().unwrap().send((node.id, msg.clone()));
    });
}
