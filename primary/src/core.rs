use bytes::Bytes;
use config::Committee;
use crypto::{PublicKey, SignatureService};
use log::{info, warn};
use std::collections::{BTreeSet, HashMap};
//use store::Store;
use tokio::sync::mpsc::{Receiver, Sender};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Instant, sleep};
use network::{CancelHandler, ReliableSender, SimpleSender};
//use crate::elections::{DbDropGuard, Election};
use config::Authority;
use crypto::Digest;
use std::convert::TryFrom;
use rand::{Rng, thread_rng};
use crypto::Hash;
use async_recursion::async_recursion;
//use serde::__private::de::Content::String;
use serde::__private::de::TagOrContentField::Tag;
use std::string::String;
use std::sync::Arc;
use crate::election::{Election, ElectionHash, Round, RoundState, Tally};
use crate::general::{PrimaryMessage, QUORUM, SEMI_QUORUM};
use crate::node::NodeId;
use crate::vote::{Decision, PrimaryVote, ValidationStatus, Value};
use crate::vote::ValidationStatus::{Invalid, Pending, Valid};
use crate::vote::Value::{One, Zero};
use crate::vote::VoteType::{Commit, Decide, InitialVote};

//#[cfg(test)]
//#[path = "tests/batch_maker_tests.rs"]
//pub mod batch_maker_tests;

/// Assemble clients transactions into batches.
pub struct Core {
    /// The public key of this primary.
    name: PublicKey,
    /// Service to sign votes.
    signature_service: SignatureService,
    /// Committee
    committee: Committee,
    /// The preferred batch size (in bytes).
    batch_size: usize,
    /// The maximum delay after which to seal the batch (in ms).
    max_batch_delay: u64,
    // Channel to receive transactions from the network.
    //rx_transaction: Receiver<Vec<Transaction>>,
    /// The network addresses of the other primaries.
    primary_addresses: Vec<(PublicKey, SocketAddr)>,
    // Holds the current batch.
    //current_batch: Batch,
    /// Holds the size of the current batch (in bytes).
    current_batch_size: usize,
    /// A network sender to broadcast the batches to the other workers.
    network: ReliableSender,
    // Election container
    //elections: Election,
    // Decided txs
    //decided_txs: HashMap<BlockHash, HashMap<PublicKey, usize>>,
    /// Network delay
    network_delay: u64,
    counter: u64,
    rx_votes: Receiver<PrimaryVote>,
    votes: HashMap<Round, Tally>,
    current_round: usize,
    //rx_decisions: Receiver<(BlockHash, PublicKey, usize)>,
    /// Keeps the cancel handlers of the messages we sent.
    cancel_handlers: HashMap<Round, Vec<CancelHandler>>,
    //current_tx: BlockHash,
    rounds_expired: BTreeSet<usize>,

    elections: HashMap<ElectionHash, Election>,
    id: NodeId,
    //sender: Arc<Mutex<Sender<(NodeId, Message)>>>,
    decided: HashMap<ElectionHash, Value>,
    messages: u64,
    byzantine: bool,
    sent_votes: BTreeSet<PrimaryVote>,
}

/*#[derive(Debug, Clone)]
pub struct VoteDecision {
    decision: usize,
    proof: BTreeSet<PrimaryVote>,
    decision_type: VoteType,
    decided: bool,
}

impl VoteDecision {
    pub fn new(decision: usize, proof: BTreeSet<PrimaryVote>, decision_type: VoteType, decided: bool) -> Self {
        Self { decision, proof, decision_type, decided }
    }
}

pub type Round = usize;

#[derive(Debug, Clone)]
pub struct Tally {
    votes: BTreeSet<PrimaryVote>,
    voted: bool,
    weak_zeros: usize,
    weak_ones: usize,
    strong_zeros: usize,
    strong_ones: usize,
}

impl Tally {
    pub fn new(votes: BTreeSet<PrimaryVote>, voted: bool, weak_zeros: usize, weak_ones: usize, strong_zeros: usize, strong_ones: usize) -> Self {
        Self {
            votes, voted, weak_zeros, weak_ones, strong_zeros, strong_ones,
        }
    }
}*/

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        signature_service: SignatureService,
        committee: Committee,
        batch_size: usize,
        max_batch_delay: u64,
        //rx_transaction: Receiver<Vec<Transaction>>,
        primary_addresses: Vec<(PublicKey, SocketAddr)>,
        rx_votes: Receiver<PrimaryVote>,
        byzantine: bool,
        //rx_decisions: Receiver<(BlockHash, PublicKey, usize)>,
        id: NodeId,
    ) {
        tokio::spawn(async move {
            Self {
                name,
                signature_service,
                committee,
                batch_size,
                max_batch_delay,
                //rx_transaction,
                primary_addresses,
                //current_batch: Batch::with_capacity(batch_size * 2),
                current_batch_size: 0,
                network: ReliableSender::new(),
                //elections: HashMap::new(),
                //decided_txs: HashMap::new(),
                network_delay: 200,
                counter: 0,
                rx_votes,
                votes: HashMap::new(),
                current_round: 0,
                //rx_decisions,
                //db: DbDropGuard::new(),
                cancel_handlers: HashMap::new(),
                //current_tx: BlockHash(Digest([0 as u8; 32])),
                rounds_expired: BTreeSet::new(),
                elections: HashMap::new(),
                decided: HashMap::new(),
                messages: 0,
                byzantine,
                sent_votes: BTreeSet::new(),
                id,
            }
            .run()
            .await;
        });
    }

    /// Broadcast message
    async fn broadcast_message(&mut self, message: PrimaryMessage, addresses: Vec<SocketAddr>, round: Round) {
        let serialized = bincode::serialize(&message).expect("Failed to serialize our own vote");
        let bytes = Bytes::from(serialized.clone());
        //if !addresses.is_empty() {
            let delay = rand::thread_rng().gen_range(0..1000) as u64;
            sleep(Duration::from_millis(delay)).await;
            info!("Message {:?} sent to {:?} with a delay of {:?} ms", message, addresses, delay);
            let handlers = self.network.broadcast(addresses, bytes.clone()).await;
        //}
        self.cancel_handlers
            .entry(round)
            .or_insert_with(Vec::new)
            .extend(handlers);
    }

    /*/// Tally vote
    async fn tally_vote(&self, vote: PrimaryVote) -> Tally {
        let mut new_tally = Tally::new(BTreeSet::new(), false, 0, 0, 0, 0);
        match self.votes.get(&vote.round) {
            Some(tally) => {
                new_tally = Tally::new(tally.votes.clone(), tally.voted, tally.weak_zeros, tally.weak_ones, tally.strong_zeros, tally.strong_ones);
                new_tally.votes.insert(vote.clone());
                if vote.author == self.name {
                    new_tally.voted = true;
                }
                if vote.decision == 0 && vote.vote_type == VoteType::Weak {
                    new_tally.weak_zeros += 1;
                }
                else if vote.decision == 1 && vote.vote_type == VoteType::Weak {
                    new_tally.weak_ones += 1;
                }
                else if vote.decision == 0 && vote.vote_type == VoteType::Strong {
                    new_tally.strong_zeros += 1;
                }
                else if vote.decision == 1 && vote.vote_type == VoteType::Strong {
                    new_tally.strong_ones += 1;
                }
                //return new_tally;
            }
            None => {
                let mut votes = BTreeSet::new();
                votes.insert(vote.clone());
                let mut voted = false;
                if vote.author == self.name {
                    voted = true;
                }
                if vote.decision == 0 && vote.vote_type == VoteType::Weak {
                    new_tally = Tally::new(votes, voted, 1, 0, 0, 0);
                }
                else if vote.decision == 1 && vote.vote_type == VoteType::Weak {
                    new_tally = Tally::new(votes, voted, 0, 1, 0, 0);
                }
                else if vote.decision == 0 && vote.vote_type == VoteType::Strong {
                    new_tally = Tally::new(votes, voted, 0, 0, 1, 0);
                }
                else if vote.decision == 1 && vote.vote_type == VoteType::Strong {
                    new_tally = Tally::new(votes, voted, 0, 0, 0, 1);
                }
            }
        }
        info!("New tally in round {}: {:?}", vote.round, new_tally.clone());
        new_tally
    }*/

    /*async fn make_decision(&self, round: usize) -> VoteDecision {
        let mut decision = VoteDecision::new(0, BTreeSet::new(), VoteType::Weak, false);
        match self.decided_txs.get(&self.current_tx) {
            Some(a) => {
                match a.get(&self.name) {
                    Some(d) => decision.decision = *d,
                    None => ()
                }
                // proof of decision missing
                decision.decision_type = VoteType::Strong;
                decision.decided = true;
                return decision;
            }
            None => ()
        }
        match self.votes.get(&round) {
            Some(tally) => {
                decision.proof = self.votes.get(&round).unwrap().votes.clone();
                if tally.strong_ones >= 3 {
                    decision.decision = 1;
                    decision.decision_type = VoteType::Strong;
                    decision.decided = true;
                    info!("Confirmed {:?} in round {}!", self.votes.get(&round).unwrap().votes.first().unwrap().tx.clone(), round);
                }
                else if tally.strong_zeros >= 3 {
                    decision.decision_type = VoteType::Strong;
                    decision.decided = true;
                    info!("Rejected {:?} in round {}!", self.votes.get(&round).unwrap().votes.first().unwrap().tx.clone(), round);
                }
                else if tally.weak_zeros + tally.strong_zeros >= 3 {
                    decision.decision_type = VoteType::Strong;
                }
                else if tally.weak_ones + tally.strong_ones >= 3 {
                    decision.decision = 1;
                    decision.decision_type = VoteType::Strong;
                }
                else if tally.weak_zeros + tally.strong_zeros < tally.weak_ones + tally.strong_ones {
                    decision.decision = 1;
                }
            }
            None => {
                let random_decision = rand::thread_rng().gen_range(0..2) as usize;
                return VoteDecision::new(random_decision, BTreeSet::new(), VoteType::Weak, false);
            }
        }
        info!("Decided in round {}: {:?}", round, decision.decided);
        decision
    }*/

    pub async fn handle_vote(&mut self, vote: &PrimaryVote) {
        //let mut rng = thread_rng();
        //let destination = rng.gen_range(0, NUMBER_OF_TOTAL_NODES) as u64;
        let destination = 2;
        if !self.sent_votes.contains(&vote) {
            //self.send_vote(vote.clone(), destination);
        }
        self.sent_votes.insert(vote.clone());
        info!("{:?} received {:?}", self.id, vote.clone());
        let mut election = Election::new(vote.election_hash.clone());
        let mut round_state = RoundState::new(vote.election_hash.clone());
        // only if vote is valid
        if let Some(e) = self.elections.get(&vote.election_hash) {
            election = e.clone();
            if let Some(rs) = election.state.get(&vote.round) {
                round_state = rs.clone();
            }
            //else {
            //Self::set_timeout(self.id, self.sender.clone(), vote.clone());
            //}
        }
        else {
            //Self::set_timeout(self.id, self.sender.clone(), vote.clone());
        }
        if !round_state.validated_votes.contains_key(&vote.vote_hash.clone()) {
            match self.validate_vote(vote.clone()).await {
                Valid => {
                    info!("{:?} is valid!", vote.clone());
                    if vote.signer == self.id {
                        round_state.voted = true;
                    }
                    round_state.tally_vote(vote.clone());
                    round_state.validated_votes.insert(vote.vote_hash.clone(), vote.clone());
                    //round_state = self.validate_pending_votes(vote.clone(), round_state.clone());
                    if !round_state.voted && vote.round.0 == 0 {
                        let own_vote = PrimaryVote::random(self.id, vote.election_hash.clone());
                        round_state.validated_votes.insert(own_vote.vote_hash.clone(), own_vote.clone());
                        round_state.tally_vote(own_vote.clone());

                        //self.send_vote(own_vote.clone(), destination);
                        round_state.voted = true;
                    }
                    election.state.insert(vote.round, round_state.clone());

                    let next_round = Round(vote.round.0 + 1);
                    let mut next_round_state = RoundState::new(vote.election_hash.clone());
                    if let Some(rs) = election.state.get(&next_round) {
                        next_round_state = rs.clone();
                    }
                    if round_state.validated_votes.len() >= QUORUM && !next_round_state.voted && round_state.timed_out {
                        let decision = self.decide_vote(round_state.tally.clone()).await;
                        let vote_hash = PrimaryVote::vote_hash(next_round, decision.value.clone(), self.id, decision.vote_type.clone());
                        let proof = Some(round_state.validated_votes.iter().map(|(hash, vote)| hash.clone()).collect());
                        let next_round_vote = PrimaryVote::new(self.id, vote_hash, next_round, decision.value.clone(), decision.vote_type.clone(), proof, vote.election_hash.clone());
                        next_round_state.validated_votes.insert(next_round_vote.vote_hash.clone(), next_round_vote.clone());
                        next_round_state.tally_vote(next_round_vote.clone());
                        next_round_state.voted = true;
                        //Self::set_timeout(self.id, self.sender.clone(), next_round_vote.clone());
                        //self.send_vote(next_round_vote.clone(), destination);
                        if next_round_vote.vote_type == Decide {
                            info!("{:?} decided {:?} in {:?} of {:?}", self.id, next_round_vote.value, next_round_vote.round, next_round_vote.election_hash);
                            election.is_decided = true;
                            self.decided.insert(next_round_vote.election_hash.clone(), next_round_vote.value.clone());
                        }
                        if !election.is_decided {
                            election.state.insert(next_round, next_round_state.clone());
                        }
                        else {
                            self.elections.remove(&election.hash);
                        }
                    }
                },
                Invalid => {
                    info!("{:?} is invalid!", vote.clone());
                    return;
                },
                Pending => {
                    info!("{:?} is pending!", vote.clone());
                    round_state.unvalidated_votes.insert(vote.clone(), vote.proof.as_ref().unwrap().clone());
                }
            }
        }
        self.elections.insert(vote.election_hash.clone(), election.clone());
        info!("State of election of node {:?}: {:?}", self.id, election);
    }

    async fn validate_vote(&mut self, vote: PrimaryVote) -> ValidationStatus {
        if vote.round != Round(0) {
            if let Some(e) = self.elections.get(&vote.election_hash.clone()) {
                if let Some(rs) = e.state.get(&Round(vote.round.0 - 1)) {
                    let mut votes = BTreeSet::new();
                    for hash in vote.clone().proof.unwrap().iter() {
                        if !rs.validated_votes.iter().any(|(h, v)| h == hash) {
                            return Pending;
                        }
                        else {
                            let mut iter = rs.validated_votes.iter().filter(|(h, v)| h == &hash);
                            votes.insert(iter.next().unwrap().1.clone());
                        }
                    }
                    let tally = Tally::from_votes(votes.clone());
                    let mut proof_tally = BTreeSet::new();
                    let proof = vote.proof.as_ref().unwrap().clone();
                    for vote in proof {
                        proof_tally.insert(rs.validated_votes.get(&vote).unwrap().clone());
                    }
                    //let p = Tally::from_votes(proof_tally.clone());
                    let decision = self.decide_vote(tally.clone()).await;
                    if decision.vote_type == vote.vote_type && decision.value == vote.value {
                        return Valid;
                    }
                    else {
                        info!("Vote has proof but the decision is wrong!");
                        return Invalid;
                    }
                }
            }
        }
        else {
            if vote.vote_type == Commit || vote.vote_type == Decide {
                return Invalid;
            }
            else {
                return Valid;
            }
        }
        Invalid
    }

    pub async fn decide_vote(&mut self, tally: Tally) -> Decision {
        let mut decision = Decision::new(Zero, InitialVote);
        if self.byzantine {
            return Decision::random();
        }
        if tally.zero_commits >= SEMI_QUORUM as u64 && tally.one_commits >= SEMI_QUORUM as u64 {
            info!("This should not happen!!!");
        }
        else if tally.one_decides > 0 {
            decision.vote_type = Decide;
            decision.value = One;
        }
        else if tally.zero_decides > 0 {
            decision.vote_type = Decide;
        }
        else if tally.zero_commits >= QUORUM as u64 {
            //info!("Node {:?} decided value {:?}", self.id, Zero);
            decision.vote_type = Decide;
        }
        else if tally.one_commits >= QUORUM as u64 {
            //info!("Node {:?} decided value {:?}", self.id, One);
            decision.value = One;
            decision.vote_type = Decide;
        }
        else if tally.zero_votes + tally.zero_commits >= QUORUM as u64 || tally.zero_commits >= SEMI_QUORUM as u64 {
            decision.vote_type = Commit;
        }
        else if tally.one_votes + tally.one_commits >= QUORUM as u64 || tally.one_commits >= SEMI_QUORUM as u64 {
            decision.value = One;
            decision.vote_type = Commit;
        }
        else if tally.zero_votes + tally.zero_commits >= SEMI_QUORUM as u64 {
            decision.vote_type = Commit;
        }
        else {
            decision.value = One;
        }
        decision
    }

    /// Main loop receiving incoming transactions and creating batches.
    pub async fn run(&mut self) {
        let (public_keys, mut addresses): (Vec<PublicKey>, Vec<SocketAddr>) = self.primary_addresses.iter().cloned().unzip();
        let timer = sleep(Duration::from_millis(self.max_batch_delay));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                Some(vote) = self.rx_votes.recv() => self.handle_vote(&vote).await,

                //Some(Message::TimerExpired(vote)) => self.handle_timeout(vote),

                /*Some(decision) = self.rx_decisions.recv() => {
                    info!("Received a decision: {:#?}", decision);
                    match self.decided_txs.get(&decision.0.clone()) {
                        Some(a) => {
                            let mut hp = a.clone();
                            hp.insert(decision.1, decision.2);
                            self.decided_txs.insert(decision.0.clone(), hp);
                        }
                        None => {
                            let mut hp = HashMap::new();
                            hp.insert(decision.1, decision.2);
                            self.decided_txs.insert(decision.0.clone(), hp);
                        }
                    }
                    info!("Decisions: {:?}", self.decided_txs.clone());
                    if self.decided_txs.get(&decision.0).unwrap().len() >= 3 {
                        info!("public keys: {:?}", &public_keys);
                        let decisions = self.decided_txs.get(&self.current_tx).unwrap();
                        for i in 0..decisions.len() - 1 {
                            assert_eq!(decisions.get(&self.name), decisions.get(&public_keys[i]));
                        }
                        info!("CONSENSUS ACHIEVED!!!");
                    }
                }

                Some(transactions) = self.rx_transaction.recv() => {
                    /*for transaction in transactions {
                        info!("Received tx {:?}", transaction.digest().0);
                        self.current_tx = transaction.digest();
                        //self.db.db().set(transaction.digest(), 0, Some(Duration::from_millis(500))).await;
                        //info!("Elections: {:?}", &self.db.db());

                        /// Initial random vote
                        let decision = rand::thread_rng().gen_range(0..2);
                        let first_own_vote = PrimaryVote::new(transaction.digest(), decision, &self.name, &self.name, &mut self.signature_service, 0, BTreeSet::new(), VoteType::Weak).await;
                        let mut new_tally = Tally::new(BTreeSet::new(), false, 0, 0, 0, 0);

                        match self.votes.get(&0) {
                            Some(tally) => {
                                new_tally = tally.clone();
                                let decision = self.make_decision(first_own_vote.round).await;
                                if decision.decided {
                                    match self.decided_txs.get(&first_own_vote.tx.clone()) {
                                        Some(a) => {
                                            let mut hp = a.clone();
                                            hp.insert(self.name, decision.decision);
                                            self.decided_txs.insert(first_own_vote.tx.clone(), hp);
                                        }
                                        None => {
                                            let mut hp = HashMap::new();
                                            hp.insert(self.name, decision.decision);
                                            self.decided_txs.insert(first_own_vote.tx.clone(), hp);
                                        }
                                    }
                                    self.broadcast_message(PrimaryMessage::Decision((first_own_vote.tx.clone(), self.name, decision.decision)), addresses.clone(), first_own_vote.round).await;
                                }
                            }
                            None => (),
                        }
                        new_tally = self.tally_vote(first_own_vote.clone()).await;
                        self.votes.insert(first_own_vote.round, new_tally);
                        if self.byzantine_node {
                            for address in &addresses {
                                let random_decision = rand::thread_rng().gen_range(0..2);
                                let mut proof = BTreeSet::new();
                                if self.current_round != 0 {
                                    proof = self.votes.get(&self.current_round).unwrap().votes.clone();
                                }
                                let byzantine_vote = PrimaryVote::new(transaction.digest(), random_decision, &self.name, &self.name, &mut self.signature_service, 0, proof, VoteType::Weak).await;
                                self.broadcast_message(PrimaryMessage::Vote(byzantine_vote), vec![*address], self.current_round).await;
                                info!("Voted in round {}", self.current_round);
                            }
                        }
                        else {
                            self.broadcast_message(PrimaryMessage::Vote(first_own_vote.clone()), addresses.clone(), first_own_vote.round).await;
                            info!("Voted in round {}", self.current_round);
                        }
                        //self.current_round += 1;
                        //self.db.db().set(transaction.digest(), 0, Some(Duration::from_millis(500))).await;
                        //info!("Elections: {:?}", &self.db.db());
                    }*/
                }*/

                //Some(vote) = self.rx_votes.recv() => {
                    /*info!("Received a vote: {:#?}", vote);
                    let mut len = 0;
                    match self.decided_txs.get(&self.current_tx.clone()) {
                        Some(a) => {
                            len = a.len();
                        }
                        None => ()
                    }
                    if self.byzantine_node && len <= 3 {
                        for address in &addresses {
                            let random_decision = rand::thread_rng().gen_range(0..2);
                            // valid proof
                            let byzantine_vote = PrimaryVote::new(vote.tx.clone(), random_decision, &self.name, &self.name, &mut self.signature_service, 0, BTreeSet::new(), VoteType::Weak).await;
                            self.broadcast_message(PrimaryMessage::Vote(byzantine_vote), vec![*address], vote.round).await;
                        }
                    }
                    if !self.byzantine_node {
                        if len <= 2 {
                            let mut is_signature_valid = false;
                            let mut is_proof_valid = true;
                            let mut own_vote = PrimaryVote::new(vote.tx.clone(), 0, &self.name, &self.name, &mut self.signature_service, self.current_round + 1, BTreeSet::new(), VoteType::Weak).await;
                            let mut decision = VoteDecision::new(0, BTreeSet::new(), VoteType::Weak, false);
                            let mut next_round_tally = Tally::new(BTreeSet::new(), false, 0, 0, 0, 0);
                            let mut new_tally = Tally::new(BTreeSet::new(), false, 0, 0, 0, 0);
                            match self.votes.get(&vote.round) {
                                Some(tally) => {
                                    match tally.votes.iter().find(|x| x.author == vote.author) {
                                        Some(v) => info!("Vote of node {} in round {} was already tallied!", vote.author, vote.round),
                                        None => {
                                            /// Validate signature
                                            match vote.signature.verify(&vote.digest(), &vote.author) {
                                                Ok(()) => {
                                                    info!("Signature of vote {} is valid!", &vote.digest());
                                                    is_signature_valid = true;
                                                }
                                                Err(e) => info!("Signature of vote {} is not valid!", &vote.digest()),
                                            }
                                            if vote.vote_type == Justify {
                                                is_proof_valid = self.validate_proof(vote.clone()).await;
                                            }
                                        }
                                    }
                                }
                                None => {
                                    /// Validate signature
                                    match vote.signature.verify(&vote.digest(), &vote.author) {
                                        Ok(()) => {
                                            info!("Signature of vote {} is valid!", &vote.digest());
                                            is_signature_valid = true;
                                        }
                                        Err(e) => info!("Signature of vote {} is not valid!", &vote.digest()),
                                    }
                                    is_proof_valid = self.validate_proof(vote.clone()).await;
                                }
                            }
                            if is_signature_valid && is_proof_valid {
                                new_tally = self.tally_vote(vote.clone()).await;
                                self.votes.insert(vote.round, new_tally.clone());
                            match self.votes.get(&(self.current_round + 1)) {
                                Some(tally) => next_round_tally = tally.clone(),
                                None => (),
                            }
                                if !next_round_tally.voted && new_tally.votes.len() == 4 || (self.rounds_expired.contains(&self.current_round) && !next_round_tally.voted && new_tally.votes.len() >= 3) {
                                    if self.decided_txs.contains_key(&vote.tx.clone()) {
                                        match self.decided_txs.get(&vote.tx.clone()).unwrap().get(&self.name) {
                                            Some(d) => {
                                                decision.decided = true;
                                                decision.decision = *d;
                                                decision.decision_type = VoteType::Strong;
                                                decision.proof = self.votes.get(&(vote.round)).unwrap().votes.clone();
                                            }
                                            None => {
                                                decision = self.make_decision(vote.round).await;
                                            }
                                        }
                                    }
                                    else {
                                        decision = self.make_decision(vote.round).await;
                                    }
                                    own_vote.decision = decision.decision;
                                    own_vote.proof = decision.proof;
                                    own_vote.vote_type = decision.decision_type;
                                    self.broadcast_message(PrimaryMessage::Vote(own_vote.clone()), addresses.clone(), own_vote.round).await;
                                    next_round_tally = self.tally_vote(own_vote.clone()).await;
                                    self.votes.insert(own_vote.round, next_round_tally);
                                    /// Reset timer
                                    timer.as_mut().reset(Instant::now() + Duration::from_millis(self.max_batch_delay));
                                    info!("Voted in round {}", self.current_round + 1);
                                    self.current_round += 1;
                                    //self.db.db().set(own_vote.tx.clone(), own_vote.round + 1, Some(Duration::from_millis(500))).await;
                                }
                                else {
                                    decision = self.make_decision(vote.round).await;
                                }
                                if decision.decided {
                                    match self.decided_txs.get(&vote.tx.clone()) {
                                        Some(a) => {
                                            let mut hp = a.clone();
                                            hp.insert(self.name, decision.decision);
                                            self.decided_txs.insert(vote.tx.clone(), hp);
                                        }
                                        None => {
                                            let mut hp = HashMap::new();
                                            hp.insert(self.name, decision.decision);
                                            self.decided_txs.insert(vote.tx.clone(), hp);
                                        }
                                    }
                                    self.broadcast_message(PrimaryMessage::Decision((vote.tx.clone(), self.name, decision.decision)), addresses.clone(), vote.round).await;
                                    let decisions = self.decided_txs.get(&self.current_tx).unwrap();
                                    info!("Decisions: {:?}", decisions.clone());
                                    if decisions.len() == 3 {
                                        info!("public keys: {:?}", &public_keys);
                                        for i in 0..decisions.len() - 1 {
                                            assert_eq!(decisions.get(&self.name), decisions.get(&public_keys[i]));
                                        }
                                        info!("CONSENSUS ACHIEVED!!!");
                                    }
                                }
                            }
                        }
                    }*/
                //}
            };

            // Give the change to schedule other tasks.
            tokio::task::yield_now().await;
        }
    }
}

pub fn now() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let in_ms = since_the_epoch.as_secs() * 1000 +
        since_the_epoch.subsec_nanos() as u64 / 1_000_000;
    in_ms
}
