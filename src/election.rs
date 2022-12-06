use std::collections::{BTreeSet, HashMap};
use std::process::id;
use byteorder::{LittleEndian, WriteBytesExt};
use rand::{Rng, thread_rng};
use ring::digest;
use ::{Hash, Vote};
use general::QUORUM;
use NodeId;
use vote::Value::{One, Zero};
use vote::{Value, VoteHash, VoteType};

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug)]
pub(crate) struct ElectionHash(pub(crate) Hash);

impl ElectionHash {
    pub(crate) fn random() -> Self {
        let mut buf = vec![];
        let mut rng = thread_rng();
        let random = rng.gen_range(0, i64::MAX);
        buf.write_i64::<LittleEndian>(random);
        let digest = digest::digest(&digest::SHA256, &buf);
        ElectionHash(Hash(digest.as_ref().to_vec()))
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash)]
pub(crate) struct Round(pub(crate) u32);

#[derive(Debug, Clone)]
pub(crate) struct Election {
    pub(crate) hash: ElectionHash,
    pub(crate) state: HashMap<Round, RoundState>,
    pub(crate) is_decided: bool,
}

impl Election {
    pub(crate) fn new(hash: ElectionHash) -> Self {
        Election {
            hash,
            is_decided: false,
            state: HashMap::new(),
            //vote_by_hash: HashMap::new(),
            //unvalidated_votes: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RoundState {
    pub(crate) tally: Tally,
    pub(crate) voted: bool,
    //pub(crate) voted_value: Option<Vote>,
    //pub(crate) committed: bool,
    pub(crate) timed_out: bool,
    pub(crate) validated_votes: BTreeSet<Vote>,
    //pub(crate) vote_by_hash: HashMap<VoteHash, Vote>,
    pub(crate) unvalidated_votes: HashMap<Vote, BTreeSet<VoteHash>>,
    pub(crate) election_hash: ElectionHash,
}

impl RoundState {
    pub(crate) fn new(election_hash: ElectionHash) -> Self {
        Self {
            validated_votes: BTreeSet::new(),
            tally: Tally::new(),
            voted: false,
            //committed: false,
            timed_out: false,
            unvalidated_votes: HashMap::new(),
            election_hash,
        }
    }

    pub(crate) fn from(votes: BTreeSet<Vote>, tally: Tally, voted: bool, unvalidated_votes: HashMap<Vote, BTreeSet<VoteHash>>, timed_out: bool, election_hash: ElectionHash) -> Self {
        Self { validated_votes: votes, tally, voted, timed_out, unvalidated_votes, election_hash }
    }

    pub(crate) fn tally_vote(&mut self, vote: Vote) {
        if vote.vote_type == VoteType::InitialVote && vote.value == Zero {
            self.tally.zero_votes += 1;
        }
        else if vote.vote_type == VoteType::Commit && vote.value == Zero {
            self.tally.zero_commits += 1;
        }
        else if vote.vote_type == VoteType::InitialVote && vote.value == One {
            self.tally.one_votes += 1;
        }
        else if vote.vote_type == VoteType::Commit && vote.value == One {
            self.tally.one_commits += 1;
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Tally {
    pub(crate) zero_votes: u64,
    pub(crate) zero_commits: u64,
    pub(crate) one_votes: u64,
    pub(crate) one_commits: u64,
    pub(crate) zero_decides: u64,
    pub(crate) one_decides: u64,
}

impl Tally {
    pub(crate) fn new() -> Self {
        Tally {
            zero_votes: 0,
            zero_commits: 0,
            one_votes: 0,
            one_commits: 0,
            zero_decides: 0,
            one_decides: 0,
        }
    }

    pub(crate) fn from_votes(votes: BTreeSet<Vote>) -> Self {
        let mut zero_votes = 0;
        let mut zero_commits = 0;
        let mut one_votes = 0;
        let mut one_commits = 0;
        let mut one_decides = 0;
        let mut zero_decides = 0;
        for vote in votes {
            if vote.value == Zero && vote.vote_type == VoteType::InitialVote {
                zero_votes += 1;
            }
            else if vote.value == Zero && vote.vote_type == VoteType::Commit {
                zero_commits += 1;
            }
            else if vote.value == Zero && vote.vote_type == VoteType::Decide {
                zero_decides += 1;
            }
            else if vote.value == One && vote.vote_type == VoteType::InitialVote {
                one_votes += 1;
            }
            else if vote.value == One && vote.vote_type == VoteType::Commit {
                one_commits += 1;
            }
            else if vote.value == One && vote.vote_type == VoteType::Decide {
                one_decides += 1;
            }
        }
        Tally { zero_votes, zero_commits, zero_decides, one_votes, one_commits, one_decides }
    }
}

pub(crate) struct UnvalidatedVotes {
    pub(crate) hashes_not_yet_received: BTreeSet<VoteHash>,
    pub(crate) unvalidated_votes: HashMap<VoteHash, BTreeSet<VoteHash>>,
}

impl UnvalidatedVotes {
    pub(crate) fn new() -> Self {
        Self {
            hashes_not_yet_received: BTreeSet::new(),
            unvalidated_votes: HashMap::new(),
        }
    }
}