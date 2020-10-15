// Copyright 2020 IOTA Stiftung
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with
// the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on
// an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and limitations under the License.

use crate::{
    milestone::MilestoneIndex,
    packet::MessageRequest,
    protocol::{Protocol, Sender},
};

use bee_common::{shutdown_stream::ShutdownStream, worker::Error as WorkerError};
use bee_common_ext::{node::Node, packable::Packable, worker::Worker};
use bee_message::prelude::MessageId;

use async_trait::async_trait;
use futures::{select, StreamExt};
use log::{debug, info};
use tokio::time::interval;

use std::time::{Duration, Instant};

const RETRY_INTERVAL_SECS: u64 = 5;

pub(crate) struct MessageRequesterWorkerEvent(pub(crate) MessageId, pub(crate) MilestoneIndex);

pub(crate) struct MessageRequesterWorker {
    pub(crate) tx: flume::Sender<MessageRequesterWorkerEvent>,
}

async fn process_request(hash: MessageId, index: MilestoneIndex, counter: &mut usize) {
    if Protocol::get().requested_messages.contains_key(&hash) {
        return;
    }

    if process_request_unchecked(hash, index, counter).await {
        Protocol::get().requested_messages.insert(hash, (index, Instant::now()));
    }
}

/// Return `true` if the transaction was requested.
async fn process_request_unchecked(hash: MessageId, index: MilestoneIndex, counter: &mut usize) -> bool {
    if Protocol::get().peer_manager.handshaked_peers.is_empty() {
        return false;
    }

    let guard = Protocol::get().peer_manager.handshaked_peers_keys.read().await;

    for _ in 0..guard.len() {
        let epid = &guard[*counter % guard.len()];

        *counter += 1;

        if let Some(peer) = Protocol::get().peer_manager.handshaked_peers.get(epid) {
            if peer.has_data(index) {
                let mut bytes = Vec::new();
                hash.pack(&mut bytes);
                Sender::<MessageRequest>::send(epid, MessageRequest::new(&bytes));
                return true;
            }
        }
    }

    for _ in 0..guard.len() {
        let epid = &guard[*counter % guard.len()];

        *counter += 1;

        if let Some(peer) = Protocol::get().peer_manager.handshaked_peers.get(epid) {
            if peer.maybe_has_data(index) {
                let mut bytes = Vec::new();
                hash.pack(&mut bytes);
                Sender::<MessageRequest>::send(epid, MessageRequest::new(&bytes));
                return true;
            }
        }
    }

    false
}

async fn retry_requests(counter: &mut usize) {
    let mut retry_counts: usize = 0;

    for mut transaction in Protocol::get().requested_messages.iter_mut() {
        let (hash, (index, instant)) = transaction.pair_mut();
        let now = Instant::now();
        if (now - *instant).as_secs() > RETRY_INTERVAL_SECS && process_request_unchecked(*hash, *index, counter).await {
            *instant = now;
            retry_counts += 1;
        }
    }

    if retry_counts > 0 {
        debug!("Retried {} transactions.", retry_counts);
    }
}

#[async_trait]
impl<N: Node> Worker<N> for MessageRequesterWorker {
    type Config = ();
    type Error = WorkerError;

    async fn start(node: &mut N, _config: Self::Config) -> Result<Self, Self::Error> {
        let (tx, rx) = flume::unbounded();

        node.spawn::<Self, _, _>(|shutdown| async move {
            info!("Running.");

            let mut receiver = ShutdownStream::new(shutdown, rx.into_stream());

            let mut counter: usize = 0;
            let mut timeouts = interval(Duration::from_secs(RETRY_INTERVAL_SECS)).fuse();

            loop {
                select! {
                    _ = timeouts.next() => retry_requests(&mut counter).await,
                    entry = receiver.next() => match entry {
                        Some(MessageRequesterWorkerEvent(hash, index)) => process_request(hash, index, &mut counter).await,
                        None => break,
                    },
                }
            }

            info!("Stopped."); });

        Ok(Self { tx })
    }
}