use std::collections::{BTreeSet, HashMap};
use std::process::id;
use byteorder::{LittleEndian, WriteBytesExt};
use rand::{Rng, thread_rng};
use ring::digest;
use crate::general::QUORUM;
use crate::vote::Value::{One, Zero};
use crate::vote::{Value, PrimaryVote, VoteType};
use crate::general::Hash;
use serde::{Serialize, Deserialize};
use crate::{BlockHash, VoteHash};

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct ElectionHash(pub Hash);

impl ElectionHash {
    pub fn random() -> Self {
        let mut buf = vec![];
        let mut rng = thread_rng();
        let random = rng.gen_range(0..i64::MAX);
        buf.write_i64::<LittleEndian>(random);
        let digest = digest::digest(&digest::SHA256, &buf);
        ElectionHash(Hash(digest.as_ref().to_vec()))
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash, Serialize, Deserialize)]
pub struct Round(pub u32);

impl AsRef<[u8]> for Round {
    fn as_ref(&self) -> &[u8] {
        &self.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct Election {
    pub hash: BlockHash,
    pub state: HashMap<Round, RoundState>,
    pub is_decided: bool,
}

impl Election {
    pub fn new(hash: BlockHash) -> Self {
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
pub struct RoundState {
    pub tally: Tally,
    pub voted: bool,
    pub timed_out: bool,
    pub validated_votes: HashMap<VoteHash, PrimaryVote>,
    pub unvalidated_votes: HashMap<PrimaryVote, BTreeSet<VoteHash>>,
    pub election_hash: BlockHash,
}

impl RoundState {
    pub fn new(election_hash: BlockHash) -> Self {
        Self {
            validated_votes: HashMap::new(),
            tally: Tally::new(),
            voted: false,
            timed_out: false,
            unvalidated_votes: HashMap::new(),
            election_hash,
        }
    }

    pub fn from(votes: HashMap<VoteHash, PrimaryVote>, tally: Tally, voted: bool, unvalidated_votes: HashMap<PrimaryVote, BTreeSet<VoteHash>>, timed_out: bool, election_hash: BlockHash) -> Self {
        Self { validated_votes: votes, tally, voted, timed_out, unvalidated_votes, election_hash }
    }

    pub fn tally_vote(&mut self, vote: PrimaryVote) {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Tally {
    pub zero_votes: u64,
    pub zero_commits: u64,
    pub one_votes: u64,
    pub one_commits: u64,
    pub zero_decides: u64,
    pub one_decides: u64,
}

impl Tally {
    pub fn new() -> Self {
        Tally {
            zero_votes: 0,
            zero_commits: 0,
            one_votes: 0,
            one_commits: 0,
            zero_decides: 0,
            one_decides: 0,
        }
    }

    pub fn from_votes(votes: BTreeSet<PrimaryVote>) -> Self {
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

pub struct UnvalidatedVotes {
    pub hashes_not_yet_received: BTreeSet<VoteHash>,
    pub unvalidated_votes: HashMap<VoteHash, BTreeSet<VoteHash>>,
}

impl UnvalidatedVotes {
    pub fn new() -> Self {
        Self {
            hashes_not_yet_received: BTreeSet::new(),
            unvalidated_votes: HashMap::new(),
        }
    }
}