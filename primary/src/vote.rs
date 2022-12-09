use std::collections::BTreeSet;
use std::fmt::Debug;
use byteorder::{LittleEndian, WriteBytesExt};
use rand::{Rng, thread_rng};
use ring::digest;
use crate::election::{ElectionHash, Round};
use crate::general::Hash;
use crate::node::NodeId;
use crate::vote::Value::{One, Zero};
use crate::vote::VoteType::{Commit, InitialVote};
use serde::{Serialize, Deserialize};

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct VoteHash(pub Hash);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum VoteType {
    InitialVote,
    Commit,
    Decide,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub value: Value,
    pub vote_type: VoteType,
}

impl Decision {
    pub fn new(value: Value, vote_type: VoteType) -> Self {
        Decision {
            value, vote_type
        }
    }

    pub fn random() -> Self {
        let mut rng = thread_rng();
        let mut value = Zero;
        let mut vote_type = InitialVote;
        if rng.gen_range(0..2) == 1 {
            value = One;
        }
        if rng.gen_range(0..2) == 1 {
            vote_type = Commit;
        }
        Decision {
            value, vote_type
        }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PrimaryVote {
    pub signer: NodeId,
    pub vote_hash: VoteHash,
    pub round: Round,
    pub value: Value,
    pub vote_type: VoteType,
    pub proof: Option<BTreeSet<VoteHash>>,
    pub election_hash: ElectionHash,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash, Serialize, Deserialize)]
pub enum Value {
    Zero,
    One,
}

impl PrimaryVote {
    pub fn random(signer: NodeId, election_hash: ElectionHash) -> Self {
        let round = Round(0);
        let mut rng = thread_rng();
        let mut value = Zero;
        if rng.gen_range(0..2) == 1 {
            value = One;
        }
        Self {
            //origin: (),
            signer,
            vote_hash: PrimaryVote::vote_hash(round, value, signer, VoteType::InitialVote),
            round,
            value,
            //voted_value: Value::Zero,
            //committed_value: Value::Zero,
            //decided_value: Value::Zero,
            vote_type: VoteType::InitialVote,
            proof: None,
            election_hash,
        }
    }

    pub fn new(signer: NodeId, vote_hash: VoteHash, round: Round, value: Value, vote_type: VoteType, proof: Option<BTreeSet<VoteHash>>, election_hash: ElectionHash) -> Self {
        Self { signer, vote_hash, round, value, vote_type, proof, election_hash }
    }

    pub fn vote_hash(round: Round, value: Value, id: NodeId, vote_type: VoteType) -> VoteHash {
        let mut buf = vec![];
        buf.write_u32::<LittleEndian>(round.0).unwrap();
        buf.write_u64::<LittleEndian>(id.0).unwrap();
        if value == Zero {
            buf.write_u64::<LittleEndian>(0).unwrap();
        } else {
            buf.write_u64::<LittleEndian>(1).unwrap();
        }
        if vote_type == VoteType::InitialVote {
            buf.write_u64::<LittleEndian>(0).unwrap();
        } else {
            buf.write_u64::<LittleEndian>(1).unwrap();
        }
        let digest = digest::digest(&digest::SHA256, &buf);
        VoteHash(Hash(digest.as_ref().to_vec()))
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Pending,
}