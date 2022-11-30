use std::collections::BTreeSet;
use ::{NodeId, VoteHash};
use ::{Round, Value};
use ::{ElectionHash, VoteType};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Vote {
    origin: NodeId,
    signer: NodeId,
    vote_hash: VoteHash,
    round: Round,
    value: Value,
    voted_value: Value,
    committed_value: Value,
    decided_value: Value,
    vote_type: VoteType,
    proof: Option<BTreeSet<VoteHash>>,
    election_hash: ElectionHash,
}