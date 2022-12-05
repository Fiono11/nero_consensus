use std::collections::{BTreeSet, HashMap};
use ::{Hash, Vote};
use vote::Value::{One, Zero};
use vote::VoteType;
use vote::VoteType::Commit;

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
pub(crate) struct ElectionHash(pub(crate) Hash);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
pub(crate) struct Round(pub(crate) u32);

#[derive(Debug, Clone)]
pub(crate) struct Election {
    pub(crate) hash: ElectionHash,
    pub(crate) state: HashMap<Round, RoundState>,
    //vote_by_hash: HashMap<VoteHash, Vote>,
    pub(crate) is_decided: bool,
    pub(crate) unvalidated_votes: BTreeSet<Vote>,
}

impl Election {
    pub(crate) fn new(hash: ElectionHash) -> Self {
        Election {
            hash,
            is_decided: false,
            state: HashMap::new(),
            //vote_by_hash: HashMap::new(),
            unvalidated_votes: BTreeSet::new(),
        }
    }

    pub(crate) fn insert_vote(&mut self, vote: Vote, own_vote: bool) -> RoundState {
        let mut round_state = RoundState::new();
        if let Some(rs) = self.state.get(&vote.round) {
            round_state = rs.clone();
        }
        if own_vote {
            round_state.voted = true;
        }
        if vote.vote_type == Commit {
            round_state.committed = true;
        }
        round_state.votes.insert(vote.clone()).then(|| round_state = self.tally_vote(vote.clone(), round_state.clone()));
        //round_state = self.tally_vote(vote.clone(), round_state);
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

    /*fn validate_vote(&mut self, vote: Vote) {
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
    }*/
}

#[derive(Debug, Clone)]
pub(crate) struct RoundState {
    pub(crate) zero_votes: u64,
    pub(crate) zero_commits: u64,
    pub(crate) one_votes: u64,
    pub(crate) one_commits: u64,
    pub(crate) voted: bool,
    pub(crate) committed: bool,
    pub(crate) timed_out: bool,
    pub(crate) votes: BTreeSet<Vote>,
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