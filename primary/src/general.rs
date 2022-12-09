use crate::vote::PrimaryVote;
use serde::{Serialize, Deserialize};
use crate::Transaction;

pub const NUMBER_OF_BYZANTINE_NODES: usize = 2;
pub const NUMBER_OF_TOTAL_NODES: usize = 3 * NUMBER_OF_BYZANTINE_NODES + 1;
pub const QUORUM: usize = 2 * NUMBER_OF_BYZANTINE_NODES + 1;
pub const SEMI_QUORUM: usize = NUMBER_OF_BYZANTINE_NODES + 1;
pub const NUMBER_OF_TXS: usize = 1;
pub const TIMEOUT: usize = 1;

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Hash(pub Vec<u8>);

impl Hash {
    fn to_string(&self) -> String {
        hex::encode(&self.0)
    }
}

impl ::std::fmt::Display for Hash {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl ::std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum PrimaryMessage {
    SendVote(PrimaryVote),
    TimerExpired(PrimaryVote),
    Transactions(Vec<Transaction>),
}