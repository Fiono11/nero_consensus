use std::collections::BTreeSet;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::fmt::Write;
use ed25519_dalek::{Digest as _, Sha512};
use config::Committee;
use crypto::{Digest, Hash, PublicKey, Signature, SignatureService};
use serde::{Deserialize, Serialize};
use crate::{ensure, PrimaryVote};
use crate::error::{DagError, DagResult};

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize, Ord, PartialOrd)]
pub struct BlockHash(pub Digest);

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize, Ord, PartialOrd)]
pub struct ParentHash(pub Digest);

impl AsRef<[u8]> for ParentHash {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_ref()
    }
}

impl AsRef<[u8]> for BlockHash {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_ref()
    }
}

pub type Batch = Vec<Transaction>;

#[derive(Debug, Hash, PartialEq, Default, Eq, Clone, Deserialize, Serialize, Ord, PartialOrd)]
pub struct Payload(pub Vec<u8>);

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct Transaction {
    pub timestamp: u64,
    pub payload: Payload,
    pub parent: ParentHash,
    pub votes: BTreeSet<PrimaryVote>,
}

impl fmt::Debug for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tx {:?}: ", self.digest().0);
        f.debug_tuple("")
            .field(&self.timestamp)
            .field(&self.payload)
            .field(&self.parent)
            .field(&self.votes)
            .finish()
    }
}

impl Transaction {
    pub fn new() -> Self {
        Self {
            timestamp: 0,
            payload: Payload(vec![]),
            parent: ParentHash(Digest::default()),
            votes: BTreeSet::new(),
        }
    }

    pub fn payload(&self) -> Payload {
        self.payload.clone()
    }

    pub fn parent(&self) -> ParentHash {
        self.parent.clone()
    }

    pub fn votes(&self) -> BTreeSet<PrimaryVote>{
        self.votes.clone()
    }

    pub fn timestamp(&self) -> u64{
        self.timestamp
    }

    pub fn digest(&self) -> BlockHash {
        let digest = Digest(
            Sha512::digest(&self.payload.0[..]).as_slice()[..32]
                .try_into()
                .unwrap(),
        );
        BlockHash(digest)
    }
}

/*#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct PrimaryVote {
    pub tx: BlockHash,
    pub decision: usize,
    pub author: PublicKey,
    pub origin: PublicKey,
    pub signature: Signature,
    pub round: usize,
    pub proof: Option<BTreeSet<BlockHash>>,
    pub vote_type: VoteType,
}

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum VoteType {
    Weak,
    Strong,
    //Justify,
}

impl PrimaryVote {
    pub async fn new(
        id: BlockHash,
        decision: usize,
        author: &PublicKey,
        origin: &PublicKey,
        signature_service: &mut SignatureService,
        round: usize,
        proof: Option<BTreeSet<BlockHash>>,
        vote_type: VoteType,
    ) -> Self {
        let vote = Self {
            tx: id,
            decision: decision,
            author: *author,
            origin: *origin,
            signature: Signature::default(),
            round,
            proof,
            vote_type,
        };
        let signature = signature_service.request_signature(vote.digest()).await;
        Self { signature, ..vote }
    }

    pub fn verify(&self, committee: &Committee) -> DagResult<()> {
        // Ensure the authority has voting rights.
        ensure!(
            committee.stake(&self.author) > 0,
            DagError::UnknownAuthority(self.author)
        );

        // Check the signature.
        self.signature
            .verify(&self.digest(), &self.author)
            .map_err(DagError::from)
    }
}

impl Hash for PrimaryVote {
    fn digest(&self) -> Digest {
        let mut hasher = Sha512::new();
        hasher.update(&self.tx);
        //hasher.update(&self.proof);
        //hasher.update(&self.round);
        //hasher.update(&self.decision);
        hasher.update(&self.author);
        //hasher.update(&self.vote_type);
        hasher.update(&self.origin);
        //hasher.update(&self.signature);
        // add other components of the vote
        Digest(hasher.finalize().as_slice()[..32].try_into().unwrap())
    }
}

impl fmt::Debug for PrimaryVote {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: (author: {}, origin: {}, tx: {}, decision: {}, round: {}, type: {:?}, proof: {:#?})",
            self.digest(),
            self.author,
            self.origin,
            self.tx.0,
            //self.signature,
            self.decision,
            self.round,
            self.vote_type,
            self.proof,
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PrimaryMessage {
    Vote(PrimaryVote),
    //Transactions(Vec<Transaction>),
    //Decision((BlockHash, PublicKey, usize))
}*/