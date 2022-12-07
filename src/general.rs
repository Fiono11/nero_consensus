use Vote;

pub(crate) const NUMBER_OF_BYZANTINE_NODES: usize = 1;
pub(crate) const NUMBER_OF_TOTAL_NODES: usize = 3 * NUMBER_OF_BYZANTINE_NODES + 1;
pub(crate) const QUORUM: usize = 2 * NUMBER_OF_BYZANTINE_NODES + 1;
pub(crate) const SEMI_QUORUM: usize = NUMBER_OF_BYZANTINE_NODES + 1;
pub(crate) const NUMBER_OF_TXS: usize = 1;
pub(crate) const TIMEOUT: usize = 1;

#[derive(Eq, PartialEq, Clone, Ord, PartialOrd, Hash)]
pub(crate) struct Hash(pub(crate) Vec<u8>);

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

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub(crate) enum Message {
    SendVote(Vote),
    TimerExpired(Vote),
}