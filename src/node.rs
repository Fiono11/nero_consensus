use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::{clone, thread};
use std::thread::sleep;
use std::time::Duration;
use election::{Election, ElectionHash, Round, RoundState};
use general::{Message, QUORUM, SEMI_QUORUM};
use vote::{Value, Vote, vote_hash, VoteHash, VoteType};
use vote::Value::{One, Zero};
use vote::VoteType::Commit;

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
            Message::Vote(vote) => self.handle_vote(vote),
            //Message::Transaction(tx) => self.handle_transaction(tx),
            Message::TimerExpired(vote) => self.handle_timeout(vote),
        }
    }

    pub(crate) fn handle_vote(&mut self, vote: &Vote) {
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
        let mut round_state = RoundState::new();
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
        round_state.votes.insert(vote.clone()).then(|| round_state.tally_vote(vote.clone()));
        election.state.insert(vote.round, round_state);
        self.elections.insert(election.hash.clone(), election.clone());
    }

    pub(crate) fn decide_next_round_vote(&mut self, election_hash: ElectionHash, round: Round) {
        let mut election = self.elections.get(&election_hash).unwrap().clone();
        let next_round = Round(round.0 + 1);
        let mut round_state = election.state.get(&round).unwrap().clone();
        let proof: Option<BTreeSet<VoteHash>> = Some(round_state.votes.iter().map(|vote| vote.hash()).collect());
        let mut next_round_vote = Vote::new(self.id, vote_hash(next_round, Zero, self.id), next_round, Zero, VoteType::Vote, proof, election.hash.clone());

        if round_state.zero_commits >= SEMI_QUORUM as u64 && round_state.one_commits >= SEMI_QUORUM as u64 {
            info!("This should not happen!!!");
        }
        else if round_state.zero_commits >= QUORUM as u64 {
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), Zero);
            info!("Node {:?} decided value {:?}", self.id, Zero);
        }
        else if round_state.one_commits >= QUORUM as u64 {
            election.is_decided = true;
            self.decided.insert(election.hash.clone(), One);
            info!("Node {:?} decided value {:?}", self.id, One);
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

        //let next_round_state = self.insert_vote(next_round_vote.clone());
        //election.state.insert(next_round, next_round_state);
        //self.elections.insert(election.hash.clone(), election.clone());
        self.send_vote(next_round_vote.clone());
    }

    // fn first_vote

    pub(crate) fn handle_timeout(&mut self, vote: &Vote) {
        if !self.elections.contains_key(&vote.election_hash.clone()) {
            let vote = Vote::new(self.id.clone(), vote_hash(Round(0), vote.value, self.id),Round(0), vote.value,VoteType::Vote, None, vote.election_hash.clone());
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
        let msg = Message::Vote(Vote {
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

#[cfg(test)]
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
        assert!(round_state.votes.contains(&vote));
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
}
