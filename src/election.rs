use std::collections::{BTreeSet, HashMap};
use byteorder::{LittleEndian, WriteBytesExt};
use rand::{Rng, thread_rng};
use ring::digest;
use ::{Hash, Vote};
use general::QUORUM;
use vote::Value::{One, Zero};
use vote::{Value, VoteHash, VoteType};
use vote::VoteType::Commit;

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
    pub(crate) zero_votes: u64,
    pub(crate) zero_commits: u64,
    pub(crate) one_votes: u64,
    pub(crate) one_commits: u64,
    pub(crate) voted: bool,
    //pub(crate) voted_value: Option<Vote>,
    //pub(crate) committed: bool,
    pub(crate) timed_out: bool,
    pub(crate) votes: BTreeSet<Vote>,
    //pub(crate) vote_by_hash: HashMap<VoteHash, Vote>,
    pub(crate) unvalidated_votes: BTreeSet<Vote>,
    pub(crate) election_hash: ElectionHash,
}

impl RoundState {
    pub(crate) fn new(election_hash: ElectionHash) -> Self {
        Self {
            votes: BTreeSet::new(),
            zero_votes: 0,
            zero_commits: 0,
            one_votes: 0,
            one_commits: 0,
            voted: false,
            //committed: false,
            timed_out: false,
            unvalidated_votes: BTreeSet::new(),
            election_hash,
        }
    }

    pub(crate) fn from(votes: BTreeSet<Vote>, zero_votes: u64, zero_commits: u64, one_votes: u64, one_commits: u64, voted: bool, unvalidated_votes: BTreeSet<Vote>, timed_out: bool, election_hash: ElectionHash) -> Self {
        Self { votes, zero_votes, zero_commits, one_votes, one_commits, voted, timed_out, unvalidated_votes, election_hash }
    }

    pub(crate) fn tally_vote(&mut self, vote: Vote) {
        if vote.vote_type == VoteType::Vote && vote.value == Zero {
            self.zero_votes += 1;
        }
        else if vote.vote_type == VoteType::Commit && vote.value == Zero {
            self.zero_commits += 1;
        }
        else if vote.vote_type == VoteType::Vote && vote.value == One {
            self.one_votes += 1;
        }
        else if vote.vote_type == VoteType::Commit && vote.value == One {
            self.one_commits += 1;
        }
    }
}