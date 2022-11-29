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
use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex, MutexGuard};
use std::{clone, thread};
use std::process::id;
use std::thread::sleep;
use std::time::Duration;
use env_logger::{Builder, Env};
use Value::{One, Zero};
use VoteType::Commit;

const NUMBER_OF_BYZANTINE_NODES: usize = 1;
const NUMBER_OF_TOTAL_NODES: usize = 3 * NUMBER_OF_BYZANTINE_NODES + 1;
const QUORUM: usize = 2 * NUMBER_OF_BYZANTINE_NODES + 1;
const SEMI_QUORUM: usize = NUMBER_OF_BYZANTINE_NODES + 1;
const NUMBER_OF_TXS: usize = 2;
const TIMEOUT: usize = 1;

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
        let mut buf = vec![];
        let mut rng = thread_rng();
        let random = rng.gen_range(0, i64::MAX);
        buf.write_i64::<LittleEndian>(random);
        let digest = digest::digest(&digest::SHA256, &buf);
        let election_hash = ElectionHash(Hash(digest.as_ref().to_vec()));
        hashes.insert(election_hash.clone());
        let id = thread_rng().gen_range(0, net.nodes.len()) as u64;
        let vote = Vote::random(NodeId(id as u64), election_hash.clone(), Round(0));
        let mut election = Election::new(election_hash.clone());
        let mut node = net.nodes.get_mut(&(id as u64)).unwrap().lock().unwrap();
        let rs = election.insert_vote(vote.clone(), true, node.id, node.sender.clone());
        election.state.insert(vote.round, rs);
        node.elections.insert(election_hash.clone(), Arc::new(Mutex::new(election.clone())));
        node.send_vote(vote.clone());
    }

    loop {
        let mut finished = true;
        for i in NUMBER_OF_BYZANTINE_NODES..NUMBER_OF_TOTAL_NODES {
            if net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone().len() != hashes.len() {
                finished = false;
            }
        }
        if finished {
            let decided = net.nodes.get(&(NUMBER_OF_BYZANTINE_NODES as u64)).unwrap().lock().unwrap().decided.clone();
            for i in NUMBER_OF_BYZANTINE_NODES + 1..NUMBER_OF_TOTAL_NODES {
                let other_decided = net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided.clone();
                for hash in hashes.iter() {
                    if decided.get(&hash.clone()).unwrap() != other_decided.get(&hash.clone()).unwrap() {
                        info!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                        info!("decided 1: {:?}", net.nodes.get(&(NUMBER_OF_BYZANTINE_NODES as u64)).unwrap().lock().unwrap().decided);
                        info!("decided {}: {:?}", i, net.nodes.get(&(i as u64)).unwrap().lock().unwrap().decided);
                        break;
                    }
                    //assert_eq!(decided.get(&hash.clone()).unwrap(), other_decided.get(&hash.clone()).unwrap());
                }
            }
            info!("CONSENSUS ACHIEVED!!!");
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
    TimerExpired(Vote),
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
    fn random(signer: NodeId, election_hash: ElectionHash, round: Round) -> Self {
        let mut rng = thread_rng();
        let mut value = Zero;
        let mut vote_type = VoteType::Vote;
        if rng.gen_range(0, 2) == 1 {
            value = One;
        }
        if rng.gen_range(0, 2) == 1 {
            vote_type = Commit;
        }
        let mut vote = Self {
            signer,
            vote_hash: vote_hash(round, value, signer),
            round,
            value,
            vote_type: VoteType::Vote,
            proof: None,
            election_hash,
        };
        vote.proof = vote.prove();
        vote
    }

    fn prove(&self) -> Option<BTreeSet<VoteHash>> {
        None
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

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct ElectionHash(Hash);

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
struct VoteHash(Hash);

#[derive(Debug)]
struct Network {
    nodes: HashMap<u64, Arc<Mutex<Node>>>,
    receiver: Arc<Mutex<Receiver<(NodeId, Message)>>>,
}

impl Network {
    fn new() -> Self {
        let (sender, receiver) = channel();
        let mut nodes = HashMap::new();

        for i in NUMBER_OF_BYZANTINE_NODES..NUMBER_OF_TOTAL_NODES {
            let node = Arc::new(Mutex::new(Node::new(NodeId(i as u64), Arc::new(Mutex::new(sender.clone())), false)));
            nodes.insert(i as u64, node);
        }

        for i in 0..NUMBER_OF_BYZANTINE_NODES {
            let node = Arc::new(Mutex::new((Node::new(NodeId(i as u64), Arc::new(Mutex::new(sender.clone())), true))));
            nodes.insert(i as u64, node);
        }

        Network {
            nodes,
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
    committed: bool,
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
            committed: false,
            timed_out: false,
        }
    }

    fn from(votes: BTreeSet<Vote>, zero_votes: u64, zero_commits: u64, one_votes: u64, one_commits: u64, voted: bool, committed: bool, timed_out: bool) -> Self {
        Self { votes, zero_votes, zero_commits, one_votes, one_commits, voted, committed, timed_out }
    }
}

#[derive(Debug, Clone)]
struct Election {
    hash: ElectionHash,
    state: HashMap<Round, RoundState>,
    vote_by_hash: HashMap<VoteHash, Vote>,
    is_decided: bool,
    unvalidated_votes: BTreeSet<Vote>,
    decided_value: Option<Value>,
}

impl Election {
    fn new(hash: ElectionHash) -> Self {
        Election {
            hash,
            is_decided: false,
            state: HashMap::new(),
            vote_by_hash: HashMap::new(),
            unvalidated_votes: BTreeSet::new(),
            decided_value: None,
        }
    }

    fn insert_vote(&mut self, vote: Vote, own_vote: bool, id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>) -> RoundState {
        let mut round_state = RoundState::new();
        if let Some(rs) = self.state.get(&vote.round) {
            round_state = rs.clone();
        }
        else {
            set_timeout(id, sender, vote.clone());
        }
        if own_vote {
            round_state.voted = true;
        }
        if vote.vote_type == Commit {
            round_state.committed = true;
        }
        round_state.votes.insert(vote.clone()).then(|| round_state = self.tally_vote(vote.clone(), round_state.clone()));
        round_state
    }

    fn tally_vote(&mut self, vote: Vote, mut round_state: RoundState) -> RoundState {
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
        round_state
    }

    fn validate_vote(&mut self, vote: Vote, id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>) -> bool {
        // check that not only the hash exists in previous round but validate if the vote is correct
        let mut valid = true;
        if vote.round != Round(0) {
            match vote.clone().proof {
                Some(p) => {
                    for hash in vote.clone().proof.unwrap().iter() {
                        if !self.vote_by_hash.contains_key(&hash) {
                            valid = false;
                        }
                    }
                }
                None => return false,
            }

        }
        if valid {
            self.insert_vote(vote.clone(), false, id, sender);
            self.vote_by_hash.insert(vote.clone().vote_hash, vote.clone());
        }
        else {
            self.unvalidated_votes.insert(vote.clone());
        }
        valid
    }
}

#[derive(Debug, Clone)]
struct Node {
    elections: HashMap<ElectionHash, Arc<Mutex<Election>>>,
    id: NodeId,
    sender: Arc<Mutex<Sender<(NodeId, Message)>>>,
    decided: HashMap<ElectionHash, Value>,
    messages: u64,
    byzantine: bool,
}

impl Node {
    fn new(id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>, byzantine: bool) -> Self {
        Node {
            id,
            sender,
            elections: HashMap::new(),
            decided: HashMap::new(),
            messages: 0,
            byzantine,
        }
    }

    fn handle_message(&mut self, msg: &Message) {
        match msg {
            Message::Vote(vote) => self.handle_vote(vote),
            Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    fn handle_vote(&mut self, vote: &Vote) {
        info!("Node {:?} received from node {:?} {:?}", self.id, vote.signer, vote);
        let mut election = Election::new(vote.election_hash.clone());
        let mut own_vote = false;
        if self.id == vote.signer {
            own_vote = true;
        }
        if let Some(e) = self.elections.get(&vote.election_hash) {
            election = e.lock().unwrap().clone();
        }
        election.validate_vote(vote.clone(), self.id, self.sender.clone());
        let unvalidated_votes = election.clone().unvalidated_votes;
        for v in unvalidated_votes.iter() {
            if v.proof.as_ref().unwrap().contains(&vote.vote_hash) {
                election.validate_vote(v.clone(), self.id, self.sender.clone());
            }
        }
        let round_state = election.insert_vote(vote.clone(), own_vote, self.id, self.sender.clone());
        election.state.insert(vote.round, round_state.clone());
        self.elections.insert(election.hash.clone(), Arc::new(Mutex::new(election.clone())));
        info!("State of election of {:?}: {:?}", self.id, election);
        info!("number of votes in round {:?}: {:?}", vote.round.0 , election.state.get(&vote.round).unwrap().votes.len());
        if !round_state.voted && vote.round.0 == 0 {
            let own_vote = Vote::random(self.id, election.hash.clone(), Round(0));
            /*let mut own_vote = vote.clone();
            //if vote.round.0 == 0 {
                let mut rng = thread_rng();
                let bit = rng.gen_range(0, 2);
                if bit == 0 {
                    own_vote.value = Zero;
                }
                else {
                    own_vote.value = One;
                }
            //}
            own_vote.signer = self.id;
            own_vote.vote_hash = vote_hash(vote.round, vote.value, vote.signer);*/
            let rs = election.insert_vote(own_vote.clone(), true, self.id, self.sender.clone());
            election.state.insert(vote.round, rs);
            self.elections.insert(election.clone().hash, Arc::new(Mutex::new(election.clone())));
            info!("State of election of node {:?}: {:?}", self.id, election);
            info!("number of votes: {:?}", self.elections.get(&vote.election_hash).unwrap().lock().unwrap().state.get(&vote.round).unwrap().votes.len());
            self.send_vote(own_vote.clone());
        }
        let binding = self.elections.get(&vote.election_hash).unwrap().clone();
        let e = binding.lock().unwrap();
        if round_state.votes.len() >= QUORUM && round_state.timed_out {
            match election.state.get(&Round(vote.round.0 + 1)) {
                Some(rs) => {
                    if !rs.voted {
                        self.decide_next_round_vote(e, vote.round);
                    }
                }
                None => self.decide_next_round_vote(e, vote.round),
            }
            info!("State of election of node {:?}: {:?}", self.id, election);
        }
    }

    fn decide_next_round_vote(&mut self, mut election: MutexGuard<Election>, round: Round) {
        let next_round = Round(round.0 + 1);
        let mut round_state = election.state.get(&round).unwrap().clone();
        let proof: Option<BTreeSet<VoteHash>> = Some(round_state.votes.iter().map(|vote| vote.hash()).collect());
        let mut next_round_vote = Vote::new(self.id, vote_hash(next_round, Zero, self.id), next_round, Zero, VoteType::Vote, proof, election.hash.clone());
        if self.byzantine {
            let mut rng = thread_rng();
            if rng.gen_range(0, 2) == 0 {
                next_round_vote = Vote::random(self.id, election.hash.clone(), round);
                //next_round_vote.proof = proof.clone();
            }
            else {
                return;
            }
        }
        else if election.is_decided {
            next_round_vote.vote_type = Commit;
            let value = election.decided_value.unwrap();
            next_round_vote.value = value;
            next_round_vote.vote_hash = vote_hash(next_round, value, self.id);
        }
        else if round_state.zero_commits >= SEMI_QUORUM as u64 && round_state.one_commits >= SEMI_QUORUM as u64 {
            info!("This should not happen!!!");
        }
        //else if  { } if decided, always commit decided value
        else if round_state.zero_commits >= QUORUM as u64 {
            election.decided_value = Some(Zero);
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), Zero);
            info!("Node {:?} decided value {:?} in round {:?} of election {:?}", self.id, Zero, round, election.hash);
        }
        else if round_state.one_commits >= QUORUM as u64 {
            election.decided_value = Some(One);
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), One);
            info!("Node {:?} decided value {:?} in round {:?} of election {:?}", self.id, One, round, election.hash);
        }
        else if round_state.zero_votes + round_state.zero_commits >= QUORUM as u64 || round_state.zero_commits >= SEMI_QUORUM as u64 {
            next_round_vote.vote_type = Commit; // add vote_type to hash
        }
        else if round_state.one_votes + round_state.one_commits >= QUORUM as u64 || round_state.one_commits >= SEMI_QUORUM as u64 {
            next_round_vote.value = One;
            next_round_vote.vote_type = Commit;
            next_round_vote.vote_hash = vote_hash(next_round, One, self.id);
        }
        else if round_state.zero_votes + round_state.zero_commits <= SEMI_QUORUM as u64 {
            next_round_vote.value = One;
            next_round_vote.vote_hash = vote_hash(next_round, One, self.id);
        }
        else if round_state.one_votes + round_state.one_votes > SEMI_QUORUM as u64 {

        }
        else {
            info!("CASE MISSING!!!");
        }
        let next_round_state = election.insert_vote(next_round_vote.clone(), true, self.id, self.sender.clone());
        election.state.insert(next_round, next_round_state);
        self.elections.insert(election.hash.clone(), Arc::new(Mutex::new(election.clone())));
        self.send_vote(next_round_vote.clone());
    }

    fn handle_timeout(&mut self, vote: &Vote) {
        info!("{:?}: round {:?} of election {:?} timed out!", self.id, vote.round, vote.election_hash);
        let binding = self.elections.get(&vote.election_hash).unwrap().clone();
        let mut election = binding.lock().unwrap();
        let mut round_state = election.state.get(&vote.round).unwrap().clone();
        round_state.timed_out = true;
        election.state.insert(vote.round, round_state.clone());
        if round_state.votes.len() >= QUORUM && round_state.timed_out {
            match election.state.get(&Round(vote.round.0 + 1)) {
                Some(rs) => {
                    if !rs.voted { // redundant as the node should not vote in round r + 1 before round r times out
                        self.decide_next_round_vote(election, vote.round);
                    }
                }
                None => {
                    self.decide_next_round_vote(election, vote.round);
                },
            }
            //info!("State of election of node {:?}: {:?}", self.id, election);
        }
    }

    fn send_vote(&mut self, vote: Vote) {
        let msg = Message::Vote(Vote {
            signer: self.id,
            vote_hash: vote.vote_hash,
            round: vote.round,
            value: vote.value,
            vote_type: vote.vote_type.clone(),
            proof: vote.proof,
            election_hash: vote.election_hash.clone(),
        });
        self.messages += 1;
        //if self.messages < 5 {
            self.sender.lock().unwrap().send((self.id, msg));
        //}
        if vote.vote_type == Commit {
            info!("Node {:?} committed value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        else {
            info!("Node {:?} voted value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        println!("State of election of node {:?}: {:?}", self.id, self.elections.get(&vote.election_hash).unwrap());
    }
}

fn set_timeout(id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>, vote: Vote) {
    let msg = Message::TimerExpired(vote.clone());
    info!("Setting timeout...");
    thread::spawn(move || {
        sleep(Duration::from_millis(1000));
        sender.lock().unwrap().send((id, msg.clone()));
        info!("Timeout message sent!")
    });
}
