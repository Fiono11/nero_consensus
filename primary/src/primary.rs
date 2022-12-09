use std::collections::BTreeSet;
// Copyright(C) Facebook, Inc. and its affiliates.
use async_trait::async_trait;
use bytes::Bytes;
use config::{Committee, Parameters, KeyPair};
use crypto::{PublicKey, SignatureService};
use log::{info, warn};
use network::{MessageHandler, Writer, Receiver as SimpleReceiver};
use std::error::Error;
use std::process::id;
use futures::task::SpawnExt;
use store::Store;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use crate::core::Core;
use crate::error::DagError;
use crate::general::PrimaryMessage;
use crate::node::NodeId;
use crate::vote::PrimaryVote;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

pub struct Primary {
    /// The keypair of this authority.
    keypair: KeyPair,
    /// The committee information.
    committee: Committee,
    /// The configuration parameters.
    parameters: Parameters,
    /// The persistent storage.
    store: Store,
    /// If the node is byzantine or not
    byzantine_node: bool,
}

impl Primary {
    pub fn spawn(
        keypair: KeyPair,
        committee: Committee,
        parameters: Parameters,
        store: Store,
        byzantine_node: bool,
        id: NodeId,
    ) {
        // Write the parameters to the logs.
        parameters.log();

        // Define a primary instance.
        let primary = Self {
            keypair: keypair.clone(),
            committee: committee.clone(),
            parameters,
            store,
            byzantine_node,
        };

        //let (tx_transactions, rx_batch_maker) = channel(CHANNEL_CAPACITY);
        let (tx_votes, rx_votes) = channel(CHANNEL_CAPACITY);
        //let (tx_decisions, rx_decisions) = channel(CHANNEL_CAPACITY);

        // Spawn all primary tasks.
        primary.handle_messages(tx_votes);

        // The `SignatureService` is used to require signatures on specific digests.
        let signature_service = SignatureService::new(keypair.secret.clone());

        // The transactions are sent to the `BatchMaker` that assembles them into batches. It then broadcasts
        // (in a reliable manner) the batches to all other workers that share the same `id` as us. Finally, it
        // gathers the 'cancel handlers' of the messages and send them to the `QuorumWaiter`.
        Core::spawn(
            keypair.name.clone(),
            signature_service,
            committee.clone(),
            primary.parameters.batch_size,
            primary.parameters.max_batch_delay,
            //rx_batch_maker,
            committee
                .others_primaries(&keypair.name)
                .iter()
                .map(|(name, addresses)| (*name, addresses.transactions))
                .collect(),
            rx_votes,
            byzantine_node,
            id
            //rx_decisions,
        );

        if byzantine_node {
            info!("Primary {} is a byzantine node!", keypair.name.clone());
        }
        else {
            info!("Primary {} is not a byzantine node!", keypair.name.clone());
        }

        // NOTE: This log entry is used to compute performance.
        info!(
            "Primary {} successfully booted on {}",
            keypair.name.clone(),
            committee
                .primary(&keypair.name)
                .expect("Our public key or worker id is not in the committee")
                .transactions
                .ip()
        );
    }

    /// Spawn all tasks responsible to handle clients transactions.
    fn handle_messages(&self, tx_votes: Sender<PrimaryVote>) {
        // We first receive clients' transactions from the network.
        let mut address = self
            .committee
            .primary(&self.keypair.name)
            .expect("Our public key is not in the committee")
            .transactions;
        address.set_ip("127.0.0.1".parse().unwrap());
        SimpleReceiver::spawn(
            address,
            /* handler */ TxReceiverHandler { tx_votes },
        );

        info!(
            "Primary {} listening to client transactions on {}",
            self.keypair.name, address
        );
    }
}

/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler {
    //tx_batch_maker: Sender<Vec<Transaction>>,
    tx_votes: Sender<PrimaryVote>,
    //tx_decisions: Sender<(BlockHash, PublicKey, usize)>
    // only one channel for primary messages
}

#[async_trait]
impl MessageHandler for TxReceiverHandler {
    async fn dispatch(&self, _writer: &mut Writer, message: Bytes) -> Result<(), Box<dyn Error>> {
        // Send the transaction to the batch maker.
        match bincode::deserialize(&message).map_err(DagError::SerializationError)? {
            /*PrimaryMessage::Transactions(txs) => {
                //info!("Received {:?}", tx.digest().0);
                self.tx_batch_maker
                    .send(txs)
                    .await
                    .expect("Failed to send transaction")
            },*/
            PrimaryMessage::SendVote(vote) => {
                self.tx_votes
                    .send(vote)
                    .await
                    .expect("Failed to send vote")
            },
            PrimaryMessage::TimerExpired(vote) => {

            }
            /*PrimaryMessage::Decision(decision) => {
                self.tx_decisions
                    .send(decision)
                    .await
                    .expect("Failed to send decision")
            }*/
            //Err(e) => warn!("Serialization error: {}", e),
        }

        // Give the change to schedule other tasks.
        tokio::task::yield_now().await;
        Ok(())
    }
}