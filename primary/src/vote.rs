use std::collections::BTreeSet;
use std::convert::TryInto;
use std::fmt::Debug;
use byteorder::{LittleEndian, WriteBytesExt};
use ed25519_dalek::{Digest, Sha512};
use futures::AsyncWriteExt;
use log::info;
use rand::{Rng, thread_rng};
use ring::digest;
use crate::election::{ElectionHash, Round};
use crate::vote::Value::{One, Zero};
use crate::vote::VoteType::{Commit, InitialVote};
use serde::{Serialize, Deserialize};
use crypto::{Hash, PublicKey};
use crate::BlockHash;
use crypto::Digest as D;

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct VoteHash(pub D);

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum VoteType {
    InitialVote,
    Commit,
    Decide,
}

impl AsRef<[u8]> for VoteType {
    fn as_ref(&self) -> &[u8] {
        &self.as_ref()
    }
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
    pub signer: PublicKey,
    //pub vote_hash: VoteHash,
    pub round: Round,
    pub value: Value,
    pub vote_type: VoteType,
    pub proof: Option<BTreeSet<VoteHash>>,
    pub election_hash: BlockHash,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Copy, Hash, Serialize, Deserialize)]
pub enum Value {
    Zero,
    One,
}

impl AsRef<[u8]> for Value {
    fn as_ref(&self) -> &[u8] {
        &self.as_ref()
    }
}

impl PrimaryVote {
    pub async fn random(signer: PublicKey, election_hash: BlockHash) -> Self {
        let round = Round(0);
        let mut value = Zero;
        {
            let mut rng = thread_rng();
            if rng.gen_range(0..2) == 1 {
                value = One;
            }
        }
        Self {
            //origin: (),
            signer,
            //vote_hash: PrimaryVote::vote_hash(round, value, signer, VoteType::InitialVote).await,
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

    pub fn new(signer: PublicKey, round: Round, value: Value, vote_type: VoteType, proof: Option<BTreeSet<VoteHash>>, election_hash: BlockHash) -> Self {
        Self { signer, round, value, vote_type, proof, election_hash }
    }

    /*pub async fn vote_hash(round: Round, value: Value, id: PublicKey, vote_type: VoteType) -> VoteHash {
        let mut buf = vec![];
        buf.write_u32::<LittleEndian>(round.0).unwrap();
        info!("buf: {:?}", buf);
        buf.write(id);
        info!("buf1: {:?}", buf);
        //buf.write_u64::<LittleEndian>(id.0).unwrap();
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
        info!("buf2: {:?}", buf);
        let digest = digest::digest(&digest::SHA256, &buf);
        VoteHash(Hash(digest.as_ref().to_vec()))
    }*/
}

impl Hash for PrimaryVote {
    fn digest(&self) -> D {
        let mut hasher = Sha512::new();
        hasher.update(&self.signer);
        //hasher.update(&self.vote_type);
        //hasher.update(&self.round);
        //hasher.update(&self.decision);
        //hasher.update(&self.value);
        //hasher.update(&self.vote_type);
        //hasher.update(&self.round);
        //hasher.update(&self.signature);
        // add other components of the vote
        D(hasher.finalize().as_slice()[..32].try_into().unwrap())
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Pending,
}