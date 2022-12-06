use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::{clone, thread};
use std::process::id;
use std::thread::sleep;
use std::time::Duration;
use election::{Election, ElectionHash, Round, RoundState, Tally};
use general::{Message, QUORUM, SEMI_QUORUM};
use general::Message::SendVote;
use vote::{Decision, ValidationStatus, Value, Vote, VoteHash, VoteType};
use vote::ValidationStatus::{Invalid, Pending, Valid};
use vote::Value::{One, Zero};
use vote::VoteType::{Commit, Decide, InitialVote};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
pub(crate) struct NodeId(pub(crate) u64);

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) elections: HashMap<ElectionHash, Election>,
    pub(crate) id: NodeId,
    pub(crate) sender: Arc<Mutex<Sender<(NodeId, Message)>>>,
    pub(crate) decided: HashMap<ElectionHash, Value>,
    pub(crate) messages: u64,
    //votes: BTreeSet<Vote>,
}

impl Node {
    pub(crate) fn new(id: NodeId, sender: Arc<Mutex<Sender<(NodeId, Message)>>>) -> Self {
        Node {
            id,
            sender,
            elections: HashMap::new(),
            decided: HashMap::new(),
            messages: 0,
            //votes: BTreeSet::new(),
        }
    }

    pub(crate) fn handle_message(&mut self, msg: &Message) {
        match msg {
            Message::SendVote(vote) => self.handle_vote(vote),
            Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    pub(crate) fn validate_pending_votes(&mut self, vote: Vote, mut round_state: RoundState) -> RoundState {
        let mut new_unvalidated_votes = round_state.unvalidated_votes.clone();
        for (v, vs) in new_unvalidated_votes {
            if vs.contains(&vote.vote_hash) {
                let mut hp = round_state.unvalidated_votes.get(&v).unwrap().clone();
                hp.remove(&vote.vote_hash);
                if hp.is_empty() {
                    if self.validate_vote(v.clone()) == Valid {
                        let mut election = self.elections.get(&v.election_hash).unwrap().clone();
                        let mut rs = election.state.get(&v.round).unwrap().clone();
                        rs.validated_votes.insert(v.clone());
                        election.state.insert(v.round, rs);
                        self.elections.insert(v.election_hash.clone(), election);
                    }
                }
                round_state.unvalidated_votes.insert(v.clone(), hp.clone());
            }
        }
        round_state
    }

    pub(crate) fn handle_vote(&mut self, vote: &Vote) {
        let mut election = Election::new(vote.election_hash.clone());
        let mut round_state = RoundState::new(vote.election_hash.clone());
        if let Some(e) = self.elections.get(&vote.election_hash) {
            election = e.clone();
            if let Some(rs) = election.state.get(&vote.round) {
                round_state = rs.clone();
            }
            if !round_state.voted && vote.round.0 == 0 {
                let own_vote = Vote::random(self.id, vote.election_hash.clone());
                self.send_vote(own_vote.clone());
                round_state.voted = true;
            }
        }
        match self.validate_vote(vote.clone()) {
            Valid => {
                round_state.validated_votes.insert(vote.clone());
                round_state = self.validate_pending_votes(vote.clone(), round_state.clone());
                self.insert_vote(vote.clone());
                if round_state.validated_votes.len() >= QUORUM && round_state.timed_out {
                    let next_round = Round(vote.round.0 + 1);
                    let decision = self.decide_vote(round_state.tally);
                    let vote_hash = Vote::vote_hash(next_round, decision.value.clone(), self.id, decision.vote_type.clone());
                    let proof = Some(round_state.validated_votes.iter().map(|vote| vote.vote_hash.clone()).collect());
                    let next_round_vote = Vote::new(self.id, vote_hash, next_round, decision.value.clone(), decision.vote_type.clone(), proof, vote.election_hash.clone());
                    self.insert_vote(next_round_vote.clone());
                    self.send_vote(next_round_vote);
                }
            },
            Invalid => return,
            Pending => {
                round_state.unvalidated_votes.insert(vote.clone(), vote.proof.as_ref().unwrap().clone());
            }
        }
        /*info!("Node {:?} received from node {:?} {:?}", self.id, vote.signer, vote);
        let mut election = Election::new(vote.election_hash.clone());
        let mut own_vote = false;
        if self.id == vote.signer {
            own_vote = true;
        }
        if let Some(e) = self.elections.get(&vote.election_hash) {
            election = e.clone();
        }
        /*election.validate_vote(vote.clone());
        let unvalidated_votes = election.clone().unvalidated_votes;
        for v in unvalidated_votes.iter() {
            if v.proof.as_ref().unwrap().contains(&vote.vote_hash) {
                election.validate_vote(v.clone());
            }
        }*/
        //let round_state = self.insert_vote(vote.clone());
        //election.state.insert(vote.round, round_state.clone());
        //self.elections.insert(election.hash.clone(), election.clone());
        info!("State of election of node {:?}: {:?}", self.id, election);
        info!("number of votes in round {:?}: {:?}", vote.round.0 , self.elections.get(&vote.election_hash).unwrap().state.get(&vote.round).unwrap().votes.len());
        if !round_state.voted {
            let mut own_vote = vote.clone();
            own_vote.signer = self.id;
            own_vote.vote_hash = vote_hash(vote.round, vote.value, vote.signer);
            let rs = self.insert_vote(own_vote.clone());
            election.state.insert(vote.round, rs);
            self.elections.insert(election.clone().hash, election.clone());
            info!("State of election of node {:?}: {:?}", self.id, election);
            info!("number of votes: {:?}", self.elections.get(&vote.election_hash).unwrap().state.get(&vote.round).unwrap().votes.len());
            self.send_vote(own_vote.clone());
        }
        if round_state.votes.len() >= QUORUM && !election.is_decided {//&& election.clone().timed_out {
            match election.state.get(&Round(vote.round.0 + 1)) {
                Some(rs) => {
                    if !rs.voted {
                        self.decide_next_round_vote(election.hash.clone(), vote.round);
                    }
                }
                None => self.decide_next_round_vote(election.hash.clone(), vote.round),
            }
            info!("State of election of node {:?}: {:?}", self.id, election);
        }*/
    }

    pub(crate) fn insert_vote(&mut self, vote: Vote) {
        let mut round_state = RoundState::new(vote.election_hash.clone());
        let mut election = Election::new(vote.election_hash.clone());
        if let Some(e) = self.elections.get(&vote.election_hash.clone()) {
            election = e.clone();
            if let Some(rs) = election.state.get(&vote.round) {
                round_state = rs.clone();
            }
        }
        if vote.signer == self.id {
            round_state.voted = true;
        }
        if vote.vote_type == Decide {
            election.is_decided == true;
        }
        round_state.validated_votes.insert(vote.clone()).then(|| round_state.tally_vote(vote.clone()));
        election.state.insert(vote.round, round_state);
        self.elections.insert(election.hash.clone(), election.clone());
    }

    fn validate_vote(&mut self, vote: Vote) -> ValidationStatus {
        //let mut valid = true;
        if vote.round != Round(0) {
            if let Some(e) = self.elections.get(&vote.election_hash.clone()) {
                if let Some(rs) = e.state.get(&Round(vote.round.0 - 1)) {
                    println!("rs: {:?}", rs);
                    for hash in vote.clone().proof.unwrap().iter() {
                        if !rs.validated_votes.iter().any(|vote| &vote.vote_hash == hash) {
                            //valid = false;
                            return Pending;
                        }
                    }
                    let decision = self.decide_vote(rs.tally.clone());
                    if decision.vote_type == vote.vote_type && decision.value == vote.value {
                        return Valid;
                    }
                }
            }
        }
        else {
            if vote.vote_type == Commit {
                return Invalid;
            }
            else {
                return Valid;
            }
        }
        /*if valid {
            let vote1 = self.decide_vote( ,Round(vote.round.0 - 1), vote.election_hash.clone());
            if vote1 != vote {
                println!("vote1: {:?}", vote1);
                valid = false;
            }
        }*/
        Invalid
    }

    pub(crate) fn decide_vote(&mut self, tally: Tally) -> Decision {
        //let mut election = self.elections.get(&election_hash).unwrap().clone();
        //let next_round = Round(round.0 + 1);
        //let mut round_state = election.state.get(&round).unwrap().clone();
        //println!("round_state: {:?}", round_state);
        //let proof: Option<BTreeSet<VoteHash>> = Some(round_state.validated_votes.iter().map(|vote| vote.vote_hash.clone()).collect());
        //println!("proof: {:?}", proof);
        //let mut next_round_vote = Vote::new(self.id, Vote::vote_hash(next_round, Zero, self.id, VoteType::InitialVote), next_round, Zero, VoteType::InitialVote, proof, round_state.election_hash);
        let mut decision = Decision::new(Zero, InitialVote);
        if tally.zero_commits >= SEMI_QUORUM as u64 && tally.one_commits >= SEMI_QUORUM as u64 {
            info!("This should not happen!!!");
        }
        else if tally.zero_commits >= QUORUM as u64 {
            //election.is_decided = true;
            //self.decided.insert(election.hash.clone(), Zero);
            info!("Node {:?} decided value {:?}", self.id, Zero);
            decision.vote_type = Decide;
            //next_round_vote.vote_type = VoteType::Decide;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, Zero, self.id, VoteType::Decide);
        }
        else if tally.one_commits >= QUORUM as u64 {
            //election.is_decided = true;
            //self.decided.insert(election.hash.clone(), One);
            info!("Node {:?} decided value {:?}", self.id, One);
            decision.value = One;
            decision.vote_type = Decide;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, One, self.id, VoteType::Decide);
        }
        else if tally.zero_votes + tally.zero_commits >= QUORUM as u64 || tally.zero_commits >= SEMI_QUORUM as u64 {
            decision.vote_type = Commit;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, Zero, self.id, VoteType::Commit);
        }
        else if tally.one_votes + tally.one_commits >= QUORUM as u64 || tally.one_commits >= SEMI_QUORUM as u64 {
            decision.value = One;
            decision.vote_type = Commit;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, One, self.id, VoteType::Commit);
        }
        else if tally.zero_votes + tally.zero_commits >= SEMI_QUORUM as u64 {
            decision.vote_type = Commit;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, Zero, self.id, VoteType::Commit);
        }
        else {
            decision.value = One;
            //next_round_vote.vote_hash = Vote::vote_hash(next_round, One, self.id, VoteType::InitialVote);
            //info!("CASE MISSING!!!");
        }
        decision
        //next_round_vote
        //let next_round_state = self.insert_vote(next_round_vote.clone());
        //election.state.insert(next_round, next_round_state);
        //self.elections.insert(election.hash.clone(), election.clone());
        //self.send_vote(next_round_vote.clone());
    }

    // fn first_vote

    pub(crate) fn handle_timeout(&mut self, vote: &Vote) {
        if !self.elections.contains_key(&vote.election_hash.clone()) {
            let vote = Vote::new(self.id.clone(), Vote::vote_hash(Round(0), vote.value, self.id, vote.vote_type.clone()), Round(0), vote.value, VoteType::InitialVote, None, vote.election_hash.clone());
            let mut election = Election::new(vote.election_hash.clone());
            self.insert_vote(vote.clone());
            self.elections.insert(vote.election_hash.clone(), election.clone());
            self.send_vote(vote.clone());
            self.clone().set_timeout(vote.clone());
        }
        else {
            let mut election = self.elections.get(&vote.election_hash).unwrap().clone();
            //let votes = election.votes.get(&vote.round).unwrap();
            //if votes.len() > QUORUM && election.timed_out.get(&vote.round).is_some() {
                //self.decide_vote(election, vote.round);
            //}
        }
        println!("Vote: {:?}", vote);
    }

    pub(crate) fn send_vote(&mut self, vote: Vote) {
        let msg = Message::SendVote(Vote {
            signer: self.id,
            vote_hash: vote.vote_hash,
            round: vote.round,
            value: vote.value,
            //voted_value: Value::Zero,
            //committed_value: Value::Zero,
            //decided_value: Value::Zero,
            vote_type: vote.vote_type.clone(),
            proof: vote.proof,
            election_hash: vote.election_hash.clone(),
        });
        self.messages += 1;
        if self.messages < 5 {
            self.sender.lock().unwrap().send((self.id, msg));
        }
        if vote.vote_type == Commit {
            info!("Node {:?} committed value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        else {
            info!("Node {:?} voted value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        //println!("State of election of node {:?}: {:?}", self.id, self.elections.get(&vote.election_hash).unwrap());
    }

    pub(crate) fn set_timeout(self, vote: Vote) {
        let msg = Message::TimerExpired(vote.clone());
        let sender = self.sender.clone();
        thread::spawn(move || {
            sleep(Duration::from_secs(3));
            println!("Setting timeout...");
            sender.lock().unwrap().send((self.id, msg.clone()));
        });
    }
}

/*#[cfg(test)]
mod tests {
    use std::process::id;
    use Network;
    use super::*;

    #[test]
    fn insert_first_vote() {
        let mut network = Network::new();
        let mut node = network.nodes.get_mut(&0).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let vote = Vote::random(node.id, election_hash.clone());
        node.insert_vote(vote.clone());
        let election = node.elections.get(&election_hash.clone()).unwrap();
        let round_state = election.state.get(&Round(0)).unwrap();
        assert!(round_state.validated_votes.contains(&vote));
        assert!(round_state.voted);
        if vote.value == Zero {
            assert_eq!(round_state.zero_votes, 1);
            assert_eq!(round_state.zero_commits, 0);
            assert_eq!(round_state.one_votes, 0);
            assert_eq!(round_state.one_commits, 0);
        }
        else if vote.value == One {
            assert_eq!(round_state.zero_votes, 0);
            assert_eq!(round_state.zero_commits, 0);
            assert_eq!(round_state.one_votes, 1);
            assert_eq!(round_state.one_commits, 0);
        }
    }

    #[test]
    fn validate_first_round_vote() {
        let mut network = Network::new();
        let mut node = network.nodes.get_mut(&0).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value = Value::Zero;
        let vote_hash = Vote::vote_hash(round, value, node.id, VoteType::InitialVote);
        let vote = Vote::new(node.id, vote_hash, round, value, VoteType::InitialVote, None, election_hash);
        assert_eq!(node.validate_vote(vote), true);
    }

    #[test]
    fn validate_vote1() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value = Value::Zero;
        let vote_hash1 = Vote::vote_hash(round, value, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value, node3.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = Vote::vote_hash(Round(1), value, node1.id, VoteType::Commit);
        let vote = Vote::new(node1.id, vote_hash4, Round(1), value, VoteType::Commit, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), true);
    }

    #[test]
    fn validate_vote2() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::One;
        let value2 = Value::One;
        let value3 = Value::Zero;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = Vote::vote_hash(Round(1), value1, node1.id, VoteType::InitialVote);
        let vote = Vote::new(node1.id, vote_hash4, Round(1), value1, VoteType::InitialVote, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), true);
    }

    #[test]
    fn validate_vote3() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::One;
        let value2 = Value::One;
        let value3 = Value::Zero;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = Vote::vote_hash(Round(1), value1, node1.id, VoteType::Commit);
        let vote = Vote::new(node1.id, vote_hash4, Round(1), value1, VoteType::Commit, Some(proof), election_hash);
        println!("vote: {:?}", vote);
        assert_eq!(node1.validate_vote(vote), false);
    }

    #[test]
    fn validate_vote4() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let mut node4= network.nodes.get(&3).unwrap().lock().unwrap();
        let mut node5= network.nodes.get(&4).unwrap().lock().unwrap();
        let mut node6= network.nodes.get(&5).unwrap().lock().unwrap();
        let mut node7= network.nodes.get(&6).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::One;
        let value2 = Value::One;
        let value3 = Value::Zero;
        let value4 = Value::Zero;
        let value5 = Value::Zero;
        let value6 = Value::One;
        let value7 = Value::One;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote_hash4 = Vote::vote_hash(round, value3, node4.id.clone(), VoteType::InitialVote);
        let vote_hash5 = Vote::vote_hash(round, value3, node5.id.clone(), VoteType::InitialVote);
        let vote_hash6 = Vote::vote_hash(round, value3, node6.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let vote4 = Vote::new(node4.id.clone(), vote_hash4.clone(), round, value4, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote4.clone());
        let vote5 = Vote::new(node5.id.clone(), vote_hash5.clone(), round, value5, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote5.clone());
        let vote6 = Vote::new(node6.id.clone(), vote_hash6.clone(), round, value6, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote6.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        proof.insert(vote4.vote_hash);
        proof.insert(vote5.vote_hash);
        proof.insert(vote6.vote_hash);
        let vote_hash7 = Vote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote = Vote::new(node1.id, vote_hash7, Round(1), Value::Zero, VoteType::Commit, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), true);
    }

    #[test]
    /// Only a vote type Vote is valid in round 0, a Commit is not
    fn validate_vote5() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::One;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::Commit, None, election_hash.clone());
        assert_eq!(node2.validate_vote(vote1), false);
    }

    #[test]
    fn validate_vote6() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::One;
        let value2 = Value::One;
        let value3 = Value::Zero;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        //node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash.clone());
        let vote_hash4 = Vote::vote_hash(Round(1), value1, node1.id, VoteType::InitialVote);
        let vote = Vote::new(node1.id, vote_hash4, Round(1), value1, VoteType::InitialVote, Some(proof), election_hash);
        println!("vote: {:?}", vote);
        assert_eq!(node1.validate_vote(vote.clone()), false);
        node1.insert_vote(vote3.clone());
        assert_eq!(node1.validate_vote(vote), true);
    }

    #[test]
    fn validate_vote7() {
        let mut network = Network::new();
        let mut node1= network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2= network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3= network.nodes.get(&2).unwrap().lock().unwrap();
        let mut node4= network.nodes.get(&3).unwrap().lock().unwrap();
        let mut node5= network.nodes.get(&4).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value1 = Value::Zero;
        let value2 = Value::Zero;
        let value3 = Value::Zero;
        let value4 = Value::Zero;
        let value5 = Value::Zero;
        let vote_hash1 = Vote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = Vote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = Vote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote_hash4 = Vote::vote_hash(round, value3, node4.id.clone(), VoteType::InitialVote);
        let vote_hash5 = Vote::vote_hash(round, value3, node5.id.clone(), VoteType::InitialVote);
        let vote1 = Vote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = Vote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = Vote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let vote4 = Vote::new(node4.id.clone(), vote_hash4.clone(), round, value4, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote4.clone());
        let vote5 = Vote::new(node5.id.clone(), vote_hash5.clone(), round, value5, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote5.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        proof.insert(vote4.vote_hash);
        proof.insert(vote5.vote_hash);
        let vote_hash7 = Vote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote7 = Vote::new(node1.id, vote_hash7, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        assert_eq!(node1.validate_vote(vote7.clone()), true);
        let vote_hash8 = Vote::vote_hash(Round(1), Value::Zero, node2.id, VoteType::Commit);
        let vote8 = Vote::new(node2.id, vote_hash8, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash9 = Vote::vote_hash(Round(1), Value::Zero, node3.id, VoteType::Commit);
        let vote9 = Vote::new(node3.id, vote_hash9, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash10 = Vote::vote_hash(Round(1), Value::Zero, node4.id, VoteType::Commit);
        let vote10 = Vote::new(node4.id, vote_hash10, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash11 = Vote::vote_hash(Round(1), Value::Zero, node5.id, VoteType::Commit);
        let vote11 = Vote::new(node5.id, vote_hash11, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        //assert_eq!(node1.validate_vote(vote8.clone()), true);
        //assert_eq!(node1.validate_vote(vote9.clone()), true);
        //assert_eq!(node1.validate_vote(vote10.clone()), true);
        //assert_eq!(node1.validate_vote(vote11.clone()), true);
        node1.insert_vote(vote7.clone());
        node1.insert_vote(vote8.clone());
        node1.insert_vote(vote9.clone());
        node1.insert_vote(vote10.clone());
        node1.insert_vote(vote11.clone());
        //println!("{:?}", node1);
        let mut new_proof = BTreeSet::new();
        new_proof.insert(vote7.vote_hash);
        new_proof.insert(vote8.vote_hash);
        new_proof.insert(vote9.vote_hash);
        new_proof.insert(vote10.vote_hash);
        new_proof.insert(vote11.vote_hash);
        let vote_hash12 = Vote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote12 = Vote::new(node1.id, vote_hash12, Round(2), Value::Zero, VoteType::Commit, Some(new_proof.clone()), election_hash.clone());
        //println!("vote: {:?}", node1);
        assert_eq!(node1.validate_vote(vote12), true);
    }
}
*/