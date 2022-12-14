use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::{clone, thread};
use std::process::id;
use std::ptr::hash;
use std::thread::sleep;
use std::time::Duration;
use log::info;
use rand::{Rng, thread_rng};
use crate::election::{Election, ElectionHash, Round, RoundState, Tally};
use crate::general::{PrimaryMessage, QUORUM, SEMI_QUORUM, TIMEOUT};
use crate::vote::ValidationStatus::{Invalid, Pending, Valid};
use crate::vote::{Decision, ValidationStatus, Value, PrimaryVote};
use crate::vote::Value::{One, Zero};
use crate::vote::VoteType::{Commit, Decide, InitialVote};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) elections: HashMap<ElectionHash, Election>,
    pub(crate) id: NodeId,
    pub(crate) sender: Arc<Mutex<Sender<(NodeId, PrimaryMessage)>>>,
    pub(crate) decided: HashMap<ElectionHash, Value>,
    pub(crate) messages: u64,
    pub(crate) byzantine: bool,
    pub(crate) sent_votes: BTreeSet<PrimaryVote>,
}

impl Node {
    pub(crate) fn new(id: NodeId, byzantine: bool, sender: Arc<Mutex<Sender<(NodeId, PrimaryMessage)>>>) -> Self {
        Node {
            id,
            sender,
            elections: HashMap::new(),
            decided: HashMap::new(),
            messages: 0,
            byzantine,
            sent_votes: BTreeSet::new(),
        }
    }

    pub(crate) fn handle_message(&mut self, msg: &PrimaryMessage) {
        match msg {
            PrimaryMessage::SendVote(vote) => self.handle_vote(vote),
            PrimaryMessage::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    pub(crate) fn validate_pending_votes(&mut self, vote: PrimaryVote, mut round_state: RoundState) -> RoundState {
        let mut new_unvalidated_votes = round_state.unvalidated_votes.clone();
        for (v, vs) in new_unvalidated_votes {
            if vs.contains(&vote.vote_hash) {
                let mut hp = round_state.unvalidated_votes.get(&v).unwrap().clone();
                hp.remove(&vote.vote_hash);
                if hp.is_empty() {
                    if self.validate_vote(v.clone()) == Valid {
                        let mut election = self.elections.get(&v.election_hash).unwrap().clone();
                        let mut rs = election.state.get(&v.round).unwrap().clone();
                        rs.validated_votes.insert(v.vote_hash.clone(), v.clone());
                        election.state.insert(v.round, rs);
                        self.elections.insert(v.election_hash.clone(), election);
                    }
                }
                round_state.unvalidated_votes.insert(v.clone(), hp.clone());
            }
        }
        round_state
    }

    pub(crate) fn handle_vote(&mut self, vote: &PrimaryVote) {
        let mut rng = thread_rng();
        //let destination = rng.gen_range(0, NUMBER_OF_TOTAL_NODES) as u64;
        let destination = 2;
        if !self.sent_votes.contains(&vote) {
            self.send_vote(vote.clone(), destination);
        }
        self.sent_votes.insert(vote.clone());
        //info!("{:?} received {:?}", self.id, vote.clone());
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
            Self::set_timeout(self.id, self.sender.clone(), vote.clone());
        }
        if !round_state.validated_votes.contains_key(&vote.vote_hash.clone()) {
            match self.validate_vote(vote.clone()) {
                Valid => {
                    //info!("{:?} is valid!", vote.clone());
                    if vote.signer == self.id {
                        round_state.voted = true;
                    }
                    round_state.tally_vote(vote.clone());
                    round_state.validated_votes.insert(vote.vote_hash.clone(), vote.clone());
                    round_state = self.validate_pending_votes(vote.clone(), round_state.clone());
                    if !round_state.voted && vote.round.0 == 0 {
                        let own_vote = PrimaryVote::random(self.id, vote.election_hash.clone());
                        round_state.validated_votes.insert(own_vote.vote_hash.clone(), own_vote.clone());
                        round_state.tally_vote(own_vote.clone());

                        self.send_vote(own_vote.clone(), destination);
                        round_state.voted = true;
                    }
                    election.state.insert(vote.round, round_state.clone());

                    let next_round = Round(vote.round.0 + 1);
                    let mut next_round_state = RoundState::new(vote.election_hash.clone());
                    if let Some(rs) = election.state.get(&next_round) {
                        next_round_state = rs.clone();
                    }
                    if round_state.validated_votes.len() >= QUORUM && !next_round_state.voted && round_state.timed_out {
                        let decision = self.decide_vote(round_state.tally.clone());
                        let vote_hash = PrimaryVote::vote_hash(next_round, decision.value.clone(), self.id, decision.vote_type.clone());
                        let proof = Some(round_state.validated_votes.iter().map(|(hash, vote)| hash.clone()).collect());
                        let next_round_vote = PrimaryVote::new(self.id, vote_hash, next_round, decision.value.clone(), decision.vote_type.clone(), proof, vote.election_hash.clone());
                        next_round_state.validated_votes.insert(next_round_vote.vote_hash.clone(), next_round_vote.clone());
                        next_round_state.tally_vote(next_round_vote.clone());
                        next_round_state.voted = true;
                        Self::set_timeout(self.id, self.sender.clone(), next_round_vote.clone());
                        self.send_vote(next_round_vote.clone(), destination);
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
                    //info!("{:?} is invalid!", vote.clone());
                    return;
                },
                Pending => {
                    //info!("{:?} is pending!", vote.clone());
                    round_state.unvalidated_votes.insert(vote.clone(), vote.proof.as_ref().unwrap().clone());
                }
            }
        }
        self.elections.insert(vote.election_hash.clone(), election.clone());
        //info!("State of election of node {:?}: {:?}", self.id, election);
    }

    pub(crate) fn insert_vote(&mut self, vote: PrimaryVote) {
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
        round_state.validated_votes.insert(vote.vote_hash.clone(), vote.clone());
        round_state.tally_vote(vote.clone());
        election.state.insert(vote.round, round_state);
        self.elections.insert(election.hash.clone(), election.clone());
    }

    fn validate_vote(&mut self, vote: PrimaryVote) -> ValidationStatus {
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
                    let decision = self.decide_vote(tally.clone());
                    if decision.vote_type == vote.vote_type && decision.value == vote.value {
                        return Valid;
                    }
                    else {
                        //info!("Vote has proof but the decision is wrong!");
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

    pub(crate) fn decide_vote(&mut self, tally: Tally) -> Decision {
        let mut decision = Decision::new(Zero, InitialVote);
        if self.byzantine {
            return Decision::random();
        }
        if tally.zero_commits >= SEMI_QUORUM as u64 && tally.one_commits >= SEMI_QUORUM as u64 {
            //info!("This should not happen!!!");
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

    pub(crate) fn handle_timeout(&mut self, vote: &PrimaryVote) {
        //let mut rng = thread_rng();
        //let destination = rng.gen_range(0, NUMBER_OF_TOTAL_NODES) as u64;
        let destination = 2;
        //info!("{:?}: {:?} of {:?} timed out!", self.id, vote.round, vote.election_hash);
        let mut election = self.elections.get(&vote.election_hash).unwrap().clone();
        let mut round_state = election.state.get(&vote.round).unwrap().clone();
        let next_round = Round(vote.round.0 + 1);
        let mut next_round_state = RoundState::new(vote.election_hash.clone());
        if let Some(rs) = election.state.get(&next_round) {
            next_round_state = rs.clone();
        }
        round_state.timed_out = true;
        election.state.insert(vote.round, round_state.clone());
        if round_state.validated_votes.len() >= QUORUM && !next_round_state.voted && round_state.timed_out {
            let decision = self.decide_vote(round_state.tally.clone());
            let vote_hash = PrimaryVote::vote_hash(next_round, decision.value.clone(), self.id, decision.vote_type.clone());
            let proof = Some(round_state.validated_votes.iter().map(|(hash, vote)| hash.clone()).collect());
            let next_round_vote = PrimaryVote::new(self.id, vote_hash, next_round, decision.value.clone(), decision.vote_type.clone(), proof, vote.election_hash.clone());
            next_round_state.validated_votes.insert(next_round_vote.vote_hash.clone(), next_round_vote.clone());
            next_round_state.tally_vote(next_round_vote.clone());
            next_round_state.voted = true;
            Self::set_timeout(self.id, self.sender.clone(), next_round_vote.clone());
            self.send_vote(next_round_vote.clone(), destination);
            if next_round_vote.vote_type == Decide {
                //info!("{:?} decided {:?} in {:?} of {:?}", self.id, next_round_vote.value, next_round_vote.round, next_round_vote.election_hash);
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
        self.elections.insert(vote.election_hash.clone(), election.clone());
        //info!("State of election of node {:?}: {:?}", self.id, election);
        /*if !self.elections.contains_key(&vote.election_hash.clone()) {
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
        println!("Vote: {:?}", vote);*/
    }

    pub(crate) fn send_vote(&mut self, vote: PrimaryVote, destination: u64) {
        let msg = PrimaryMessage::SendVote(PrimaryVote {
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
        //if self.messages < 5 {
            self.sender.lock().unwrap().send((self.id, msg));
        //}
        if vote.vote_type == Commit {
            //info!("{:?} committed value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        else if vote.vote_type == InitialVote {
            //info!("{:?} voted value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        else if vote.vote_type == Decide {
            //info!("{:?} decided value {:?} in round {:?} of election {:?}", self.id, vote.value, vote.round, vote.election_hash.clone());
        }
        //println!("State of election of node {:?}: {:?}", self.id, self.elections.get(&vote.election_hash).unwrap());
    }

    pub(crate) fn set_timeout(id: NodeId, sender: Arc<Mutex<Sender<(NodeId, PrimaryMessage)>>>, vote: PrimaryVote) {
        let msg = PrimaryMessage::TimerExpired(vote.clone());
        thread::spawn(move || {
            sleep(Duration::from_secs(TIMEOUT as u64));
            //info!("{:?}: Timeout of {:?} of {:?} set!", id, vote.round, vote.election_hash);
            sender.lock().unwrap().send((id, msg.clone()));
        });
    }
}

#[cfg(test)]
mod tests {
    use std::process::id;
    use Network;
    use crate::network::Network;
    use crate::vote::Value::{One, Zero};
    use crate::vote::{Value, PrimaryVote, VoteType};
    use crate::vote::ValidationStatus::{Pending, Valid};
    use super::*;

    #[test]
    fn insert_first_vote() {
        let mut network = Network::new();
        let mut node = network.nodes.get_mut(&0).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let vote = PrimaryVote::random(node.id, election_hash.clone());
        node.insert_vote(vote.clone());
        let election = node.elections.get(&election_hash.clone()).unwrap();
        let round_state = election.state.get(&Round(0)).unwrap();
        assert!(round_state.validated_votes.contains_key(&vote.vote_hash.clone()));
        assert!(round_state.voted);
        if vote.value == Zero {
            assert_eq!(round_state.tally.zero_votes, 1);
            assert_eq!(round_state.tally.zero_commits, 0);
            assert_eq!(round_state.tally.one_votes, 0);
            assert_eq!(round_state.tally.one_commits, 0);
        } else if vote.value == One {
            assert_eq!(round_state.tally.zero_votes, 0);
            assert_eq!(round_state.tally.zero_commits, 0);
            assert_eq!(round_state.tally.one_votes, 1);
            assert_eq!(round_state.tally.one_commits, 0);
        }
    }

    #[test]
    fn validate_first_round_vote() {
        let mut network = Network::new();
        let mut node = network.nodes.get_mut(&0).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value = Value::Zero;
        let vote_hash = PrimaryVote::vote_hash(round, value, node.id, VoteType::InitialVote);
        let vote = PrimaryVote::new(node.id, vote_hash, round, value, VoteType::InitialVote, None, election_hash);
        assert_eq!(node.validate_vote(vote), Valid);
    }

    #[test]
    fn validate_vote1() {
        let mut network = Network::new();
        let mut node1 = network.nodes.get(&0).unwrap().lock().unwrap();
        let mut node2 = network.nodes.get(&1).unwrap().lock().unwrap();
        let mut node3 = network.nodes.get(&2).unwrap().lock().unwrap();
        let election_hash = ElectionHash::random();
        let round = Round(0);
        let value = Value::Zero;
        let vote_hash1 = PrimaryVote::vote_hash(round, value, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value, node3.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = PrimaryVote::vote_hash(Round(1), value, node1.id, VoteType::Commit);
        let vote = PrimaryVote::new(node1.id, vote_hash4, Round(1), value, VoteType::Commit, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), Valid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = PrimaryVote::vote_hash(Round(1), value1, node1.id, VoteType::InitialVote);
        let vote = PrimaryVote::new(node1.id, vote_hash4, Round(1), value1, VoteType::InitialVote, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), Valid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        let vote_hash4 = PrimaryVote::vote_hash(Round(1), value1, node1.id, VoteType::Commit);
        let vote = PrimaryVote::new(node1.id, vote_hash4, Round(1), value1, VoteType::Commit, Some(proof), election_hash);
        println!("vote: {:?}", vote);
        assert_eq!(node1.validate_vote(vote), Invalid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote_hash4 = PrimaryVote::vote_hash(round, value3, node4.id.clone(), VoteType::InitialVote);
        let vote_hash5 = PrimaryVote::vote_hash(round, value3, node5.id.clone(), VoteType::InitialVote);
        let vote_hash6 = PrimaryVote::vote_hash(round, value3, node6.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let vote4 = PrimaryVote::new(node4.id.clone(), vote_hash4.clone(), round, value4, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote4.clone());
        let vote5 = PrimaryVote::new(node5.id.clone(), vote_hash5.clone(), round, value5, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote5.clone());
        let vote6 = PrimaryVote::new(node6.id.clone(), vote_hash6.clone(), round, value6, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote6.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        proof.insert(vote4.vote_hash);
        proof.insert(vote5.vote_hash);
        proof.insert(vote6.vote_hash);
        let vote_hash7 = PrimaryVote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote = PrimaryVote::new(node1.id, vote_hash7, Round(1), Value::Zero, VoteType::Commit, Some(proof), election_hash);
        assert_eq!(node1.validate_vote(vote), Valid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::Commit, None, election_hash.clone());
        assert_eq!(node2.validate_vote(vote1), Invalid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        //node1.insert_vote(vote3.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash.clone());
        let vote_hash4 = PrimaryVote::vote_hash(Round(1), value1, node1.id, VoteType::InitialVote);
        let vote = PrimaryVote::new(node1.id, vote_hash4, Round(1), value1, VoteType::InitialVote, Some(proof), election_hash);
        println!("vote: {:?}", vote);
        assert_eq!(node1.validate_vote(vote.clone()), Pending);
        node1.insert_vote(vote3.clone());
        assert_eq!(node1.validate_vote(vote), Valid);
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
        let vote_hash1 = PrimaryVote::vote_hash(round, value1, node1.id.clone(), VoteType::InitialVote);
        let vote_hash2 = PrimaryVote::vote_hash(round, value2, node2.id.clone(), VoteType::InitialVote);
        let vote_hash3 = PrimaryVote::vote_hash(round, value3, node3.id.clone(), VoteType::InitialVote);
        let vote_hash4 = PrimaryVote::vote_hash(round, value3, node4.id.clone(), VoteType::InitialVote);
        let vote_hash5 = PrimaryVote::vote_hash(round, value3, node5.id.clone(), VoteType::InitialVote);
        let vote1 = PrimaryVote::new(node1.id.clone(), vote_hash1.clone(), round, value1, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote1.clone());
        let vote2 = PrimaryVote::new(node2.id.clone(), vote_hash2.clone(), round, value2, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote2.clone());
        let vote3 = PrimaryVote::new(node3.id.clone(), vote_hash3.clone(), round, value3, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote3.clone());
        let vote4 = PrimaryVote::new(node4.id.clone(), vote_hash4.clone(), round, value4, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote4.clone());
        let vote5 = PrimaryVote::new(node5.id.clone(), vote_hash5.clone(), round, value5, VoteType::InitialVote, None, election_hash.clone());
        node1.insert_vote(vote5.clone());
        let mut proof = BTreeSet::new();
        proof.insert(vote1.vote_hash);
        proof.insert(vote2.vote_hash);
        proof.insert(vote3.vote_hash);
        proof.insert(vote4.vote_hash);
        proof.insert(vote5.vote_hash);
        let vote_hash7 = PrimaryVote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote7 = PrimaryVote::new(node1.id, vote_hash7, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        assert_eq!(node1.validate_vote(vote7.clone()), Valid);
        let vote_hash8 = PrimaryVote::vote_hash(Round(1), Value::Zero, node2.id, VoteType::Commit);
        let vote8 = PrimaryVote::new(node2.id, vote_hash8, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash9 = PrimaryVote::vote_hash(Round(1), Value::Zero, node3.id, VoteType::Commit);
        let vote9 = PrimaryVote::new(node3.id, vote_hash9, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash10 = PrimaryVote::vote_hash(Round(1), Value::Zero, node4.id, VoteType::Commit);
        let vote10 = PrimaryVote::new(node4.id, vote_hash10, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
        let vote_hash11 = PrimaryVote::vote_hash(Round(1), Value::Zero, node5.id, VoteType::Commit);
        let vote11 = PrimaryVote::new(node5.id, vote_hash11, Round(1), Value::Zero, VoteType::Commit, Some(proof.clone()), election_hash.clone());
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
        let vote_hash12 = PrimaryVote::vote_hash(Round(1), Value::Zero, node1.id, VoteType::Commit);
        let vote12 = PrimaryVote::new(node1.id, vote_hash12, Round(2), Value::Zero, VoteType::Decide, Some(new_proof.clone()), election_hash.clone());
        //println!("vote: {:?}", node1);
        assert_eq!(node1.validate_vote(vote12), Valid);
    }
}