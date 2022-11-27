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
use std::{clone, thread};
use std::process::id;
use std::thread::sleep;
use std::time::Duration;
use Value::{One, Zero};
use VoteType::Commit;

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
        let mut buf = vec![];
        let mut rng = thread_rng();
        let random = rng.gen_range(0, i64::MAX);
        buf.write_i64::<LittleEndian>(random);
        let digest = digest::digest(&digest::SHA256, &buf);
        let election_hash = ElectionHash(Hash(digest.as_ref().to_vec()));
        hashes.insert(election_hash.clone());

        for id in 0..NUMBER_OF_TOTAL_NODES {
            let vote = Vote::random(NodeId(id as u64), election_hash.clone());
            let mut election = Election::new(election_hash.clone());
            let mut votes = BTreeSet::new();
            votes.insert(vote.clone());
            let mut round_state = RoundState::new();
            round_state.votes = votes;
            if vote.value == Zero {
                round_state.zero_votes = 1;
            }
            else {
                round_state.one_votes = 1;
            }
            election.state.insert(Round(0), round_state);
            let mut node = net.nodes.get_mut(&(id as u64)).unwrap().lock().unwrap();
            node.elections.insert(election_hash.clone(), election.clone());
            node.send_vote(vote);
        }
    }

    /*for _ in 0..NUMBER_OF_TXS {
        let tx = Transaction::random();
        println!("sending new transaction into the network {:#?}", &tx.hash());

        let id = thread_rng().gen_range(0, net.nodes.len()) as u64;
        let node = net.nodes.get_mut(&id).unwrap();
        node.lock()
            .unwrap()
            .handle_message(&Message::Transaction(tx.clone()));
        hashes.insert(tx.hash());

        //thread::sleep_ms(500);
    }*/

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
            break;
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
    //Transaction(Transaction),
    //TimerExpired(Vote),
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
struct NodeId(u64);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
struct Round(u32);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
enum Value {
    Zero,
    One,
}

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

impl Vote {
    fn random(signer: NodeId, election_hash: ElectionHash) -> Self {
        let round = Round(0);
        let mut rng = thread_rng();
        let mut value = Zero;
        if rng.gen_range(0, 2) == 1 {
            value = One;
        }
        Self {
            signer,
            vote_hash: vote_hash(round, value, signer),
            round,
            value,
            vote_type: VoteType::Vote,
            proof: None,
            election_hash,
        }
    }
}

fn vote_hash(round: Round, value: Value, id: NodeId) -> VoteHash {
    let mut buf = vec![];
    buf.write_u32::<LittleEndian>(round.0).unwrap();
    buf.write_u64::<LittleEndian>(id.0).unwrap();
    if value == Zero {
        buf.write_u64::<LittleEndian>(0).unwrap();
    }
    else {
        buf.write_u64::<LittleEndian>(1).unwrap();
    }
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
        if self.value == Zero {
            buf.write_u64::<LittleEndian>(0).unwrap();
        }
        else {
            buf.write_u64::<LittleEndian>(1).unwrap();
        }
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

/*#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Transaction {
    nonce: Value,
    data: i32,
}*/

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct ElectionHash(Hash);

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct VoteHash(Hash);

/*impl Transaction {
    fn random() -> Self {
        let mut rng = thread_rng();
        Transaction {
            nonce: Value(rng.gen_range(0, 2)),
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
}*/

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
                /*Message::TimerExpired(ref _msg) => {
                    for id in ids {
                        if id == origin.0 {
                            nodes.get_mut(&id)
                                .unwrap()
                                .lock()
                                .unwrap()
                                .handle_message(&msg)
                        }
                    }
                }*/
                // Message that timer has expired
                _ => unreachable!(),
            }
        });
    }
}

#[derive(Debug, Clone)]
struct RoundState {
    zero_votes: u64,
    zero_commits: u64,
    one_votes: u64,
    one_commits: u64,
    voted: bool,
    timed_out: bool,
    votes: BTreeSet<Vote>,
}

impl RoundState {
    fn new() -> Self {
        Self {
            votes: BTreeSet::new(),
            zero_votes: 0,
            zero_commits: 0,
            one_votes: 0,
            one_commits: 0,
            voted: false,
            timed_out: false,
        }
    }

    fn from(votes: BTreeSet<Vote>, zero_votes: u64, zero_commits: u64, one_votes: u64, one_commits: u64, voted: bool, timed_out: bool) -> Self {
        Self { votes, zero_votes, zero_commits, one_votes, one_commits, voted, timed_out }
    }
}

#[derive(Debug, Clone)]
struct Election {
    hash: ElectionHash,
    state: HashMap<Round, RoundState>,
    vote_by_hash: HashMap<VoteHash, Vote>,
    is_decided: bool,
    unvalidated_votes: BTreeSet<Vote>,
}

impl Election {
    fn new(hash: ElectionHash) -> Self {
        Election {
            hash,
            is_decided: false,
            state: HashMap::new(),
            vote_by_hash: HashMap::new(),
            unvalidated_votes: BTreeSet::new(),
        }
    }

    fn insert_vote(&mut self, vote: Vote, own_vote: bool) {
        let mut round_state = RoundState::new();
        match self.state.get(&vote.round) {
            Some(s) => {
                round_state = s.clone();
                round_state.votes.insert(vote.clone());
            }
            None => {
                round_state.votes.insert(vote.clone());
            }
        }
        if own_vote {
            round_state.voted = true;
        }
        self.state.insert(vote.round, round_state);
        self.tally_vote(vote.clone());
        self.vote_by_hash.insert(vote.clone().vote_hash, vote.clone());
    }

    fn tally_vote(&mut self, vote: Vote) {
        let round = vote.round;
        //if let Some(round_state) = self.state.get(&round) {
            let mut round_state = self.state.get(&round).unwrap().clone();
            if vote.vote_type == VoteType::Vote && vote.value == Zero {
                round_state.zero_votes += 1;
            }
            else if vote.vote_type == VoteType::Commit && vote.value == Zero {
                round_state.zero_commits += 1;
            }
            else if vote.vote_type == VoteType::Vote && vote.value == One {
                round_state.one_votes += 1;
            }
            else if vote.vote_type == VoteType::Commit && vote.value == One {
                round_state.one_commits += 1;
            }
            self.state.insert(round, round_state);
        //}
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
            self.vote_by_hash.insert(vote.clone().vote_hash, vote.clone());
        }
        else {
            self.unvalidated_votes.insert(vote.clone());
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
            //Message::Transaction(tx) => self.handle_transaction(tx),
            //Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    fn handle_vote(&mut self, vote: &Vote) {
        println!("Node {:?} received from node {:?} vote {:?}", self.id, vote.signer, vote);
        if self.decided.contains_key(&vote.election_hash) || vote.signer == self.id {
            return;
        }
        let mut election = self.elections.get(&vote.election_hash).unwrap().clone();
        /*election.validate_vote(vote.clone());
        let unvalidated_votes = election.clone().unvalidated_votes;
        for v in unvalidated_votes.iter() {
            if v.proof.as_ref().unwrap().contains(&vote.vote_hash) {
                election.validate_vote(v.clone());
            }
        }*/
        let mut round_state = RoundState::new();
        match election.state.get(&vote.round) {
            Some(rs) => {
                round_state = rs.clone();
                round_state.votes.insert(vote.clone());
                election.tally_vote(vote.clone());
                if !round_state.voted {
                    let mut own_vote = vote.clone();
                    own_vote.round = vote.round;
                    own_vote.vote_type = VoteType::Vote;
                    election.insert_vote(own_vote.clone(), true);
                    self.send_vote(own_vote.clone());
                    round_state.votes.insert(own_vote.clone());
                    election.tally_vote(own_vote.clone());
                }
                election.state.insert(vote.round, round_state.clone());
            }
            None => {
                round_state.votes.insert(vote.clone());
                election.tally_vote(vote.clone());
                let mut own_vote = vote.clone();
                own_vote.round = vote.round;
                own_vote.vote_type = VoteType::Vote;
                election.insert_vote(own_vote.clone(), true);
                self.send_vote(own_vote.clone());
                election.state.insert(vote.round, round_state.clone());
            }
        }
        self.elections.insert(election.clone().hash, election.clone());
        if round_state.votes.len() >= QUORUM && !election.is_decided {//&& election.clone().timed_out {
            self.decide_next_vote(election.hash.clone(), vote.round);
        }
        println!("State of election of node {:?}: {:?}", self.id, election);
    }

    fn decide_next_vote(&mut self, election_hash: ElectionHash, round: Round) {
        let mut election = self.elections.get(&election_hash).unwrap().clone();
        let next_round = Round(round.0 + 1);
        let mut round_state = election.state.get(&round).unwrap().clone();
        let proof: Option<BTreeSet<VoteHash>> = Some(round_state.votes.iter().map(|vote| vote.hash()).collect());
        let mut next_round_vote = Vote::new(self.id, vote_hash(next_round, Zero, self.id), Round(round.0 + 1), Zero, VoteType::Vote, proof, election.hash.clone());
        if round_state.zero_votes > QUORUM as u64 {
            next_round_vote.vote_type = Commit;
        }
        else if round_state.one_votes > QUORUM as u64 {
            next_round_vote.value = One;
            next_round_vote.vote_type = Commit;
            next_round_vote.vote_hash = vote_hash(next_round, One, self.id);
        }
        if round_state.zero_commits > QUORUM as u64 {
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), Zero);
        }
        else if round_state.one_commits > QUORUM as u64 {
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), One);
        }
        else if round_state.zero_votes + round_state.zero_commits < SEMI_QUORUM as u64 {
            next_round_vote.value = One;
            next_round_vote.vote_hash = vote_hash(next_round, One, self.id);
        }
        election.insert_vote(next_round_vote.clone(), true);
        round_state.voted = true;
        election.state.insert(next_round, round_state);
        self.elections.insert(election.hash.clone(), election.clone());
        self.send_vote(next_round_vote.clone());
    }

    // fn first_vote

    /*fn handle_timeout(&mut self, vote: &Vote) {
        if !self.elections.contains_key(&vote.election_hash.clone()) {
            let vote = Vote::new(self.id.clone(), vote_hash(Round(0), vote.value, self.id),Round(0), vote.value,VoteType::Vote, None, vote.election_hash.clone());
            let mut election = Election::new(vote.election_hash.clone());
            election.insert_vote(vote.clone(), true);
            self.elections.insert(vote.election_hash.clone(), election.clone());
            self.send_vote(vote.clone(), election);
            set_timeout(self.clone(), vote.clone());
        }
        else {
            let mut election = self.elections.get(&vote.election_hash).unwrap().clone();
            let votes = election.votes.get(&vote.round).unwrap();
            if votes.len() > QUORUM && election.timed_out.get(&vote.round).is_some() {
                self.decide_vote(election, vote.round);
            }
        }
        println!("Vote: {:?}", vote);
    }*/

    /*fn handle_transaction(&mut self, tx: &Transaction) {
        if !self.elections.contains_key(&tx.hash()) {
            let vote = Vote::new(self.id, vote_hash(Round(0), tx.nonce),Round(0),tx.nonce,VoteType::Vote, None, tx.clone().hash());
            let mut election = Election::new(vote.election_hash.clone());
            self.elections.insert(tx.hash(), election.clone());
            election.insert_vote(vote.clone(), true);
            self.send_vote(vote.clone(), election.clone());
            self.elections.insert(election.clone().hash, election.clone());
            //set_timeout(self.clone(), vote.clone());
        }
    }*/

    fn send_vote(&self, vote: Vote) {
        let msg = Message::Vote(Vote {
            signer: self.id,
            vote_hash: vote.vote_hash,
            round: vote.round,
            value: vote.value,
            vote_type: vote.vote_type,
            proof: vote.proof,
            election_hash: vote.election_hash.clone(),
        });
        self.sender.lock().unwrap().send((self.id, msg));
        println!("Node {:?} voted value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        println!("State of election of node {:?}: {:?}", self.id, vote.election_hash.clone());
    }
}

/*fn set_timeout(node: Node, vote: Vote) {
    let msg = Message::TimerExpired(vote.clone());
    let sender = node.sender.clone();
    thread::spawn(move || {
        sleep(Duration::from_secs(3));
        println!("Setting timeout...");
        sender.lock().unwrap().send((node.id, msg.clone()));
    });
}*/
