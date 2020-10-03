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
    config::ProtocolConfig,
    event::{LatestMilestoneChanged, LatestSolidMilestoneChanged},
    milestone::MilestoneIndex,
    peer::{Peer, PeerManager},
    protocol::ProtocolMetrics,
    tangle::tangle,
    worker::{
        BroadcasterWorker, BundleValidatorWorker, HasherWorker, KickstartWorker, MilestoneRequesterWorker,
        MilestoneResponderWorker, MilestoneSolidifierWorker, MilestoneSolidifierWorkerEvent, MilestoneValidatorWorker,
        PeerHandshakerWorker, ProcessorWorker, PropagatorWorker, StatusWorker, TpsWorker, TransactionRequesterWorker,
        TransactionResponderWorker,
    },
};

use bee_common_ext::{
    bee_node::BeeNode,
    event::Bus,
    node::{Node, NodeBuilder},
};
use bee_crypto::ternary::Hash;
use bee_network::{EndpointId, Network, Origin};

use dashmap::DashMap;
use futures::channel::oneshot;
use log::{debug, info};
use tokio::spawn;

use crate::worker::{MilestoneConeUpdaterWorker, TipPoolCleaner};
use std::{net::SocketAddr, ptr, sync::Arc, time::Instant};

static mut PROTOCOL: *const Protocol = ptr::null();

pub struct Protocol {
    pub(crate) config: ProtocolConfig,
    pub(crate) network: Network,
    // TODO temporary
    pub(crate) local_snapshot_timestamp: u64,
    pub(crate) bus: Arc<Bus<'static>>,
    pub(crate) metrics: ProtocolMetrics,
    pub(crate) peer_manager: PeerManager,
    pub(crate) requested_transactions: DashMap<Hash, (MilestoneIndex, Instant)>,
    pub(crate) requested_milestones: DashMap<MilestoneIndex, Instant>,
}

impl Protocol {
    pub async fn init(
        config: ProtocolConfig,
        network: Network,
        local_snapshot_timestamp: u64,
        node_builder: NodeBuilder<BeeNode>,
        bus: Arc<Bus<'static>>,
    ) -> NodeBuilder<BeeNode> {
        let protocol = Protocol {
            config,
            network: network.clone(),
            local_snapshot_timestamp,
            bus,
            metrics: ProtocolMetrics::new(),
            peer_manager: PeerManager::new(),
            requested_transactions: Default::default(),
            requested_milestones: Default::default(),
        };

        unsafe {
            PROTOCOL = Box::leak(protocol.into()) as *const _;
        }

        let (ms_send, ms_recv) = oneshot::channel();

        node_builder
            .with_worker_cfg::<HasherWorker>(Protocol::get().config.workers.transaction_worker_cache)
            .with_worker::<ProcessorWorker>()
            .with_worker::<TransactionResponderWorker>()
            .with_worker::<MilestoneResponderWorker>()
            .with_worker::<TransactionRequesterWorker>()
            .with_worker::<MilestoneRequesterWorker>()
            .with_worker_cfg::<MilestoneValidatorWorker>(Protocol::get().config.coordinator.sponge_type)
            .with_worker_cfg::<BroadcasterWorker>(network)
            .with_worker::<BundleValidatorWorker>()
            .with_worker::<PropagatorWorker>()
            .with_worker_cfg::<StatusWorker>(Protocol::get().config.workers.status_interval)
            .with_worker::<TpsWorker>()
            .with_worker_cfg::<KickstartWorker>((ms_send, Protocol::get().config.workers.ms_sync_count))
            .with_worker_cfg::<MilestoneSolidifierWorker>(ms_recv)
            .with_worker::<MilestoneConeUpdaterWorker>()
            .with_worker::<TipPoolCleaner>()
    }

    pub fn events(bee_node: &BeeNode, bus: Arc<Bus<'static>>) {
        bus.add_listener(|latest_milestone: &LatestMilestoneChanged| {
            info!(
                "New milestone {} {}.",
                *latest_milestone.0.index,
                latest_milestone
                    .0
                    .hash()
                    .iter_trytes()
                    .map(char::from)
                    .collect::<String>()
            );
            tangle().update_latest_milestone_index(latest_milestone.0.index);

            // TODO spawn ?
            Protocol::broadcast_heartbeat(
                tangle().get_latest_solid_milestone_index(),
                tangle().get_pruning_index(),
                latest_milestone.0.index,
            );
        });

        // bus.add_listener(|latest_solid_milestone: &LatestSolidMilestoneChanged| {
        //     // TODO block_on ?
        //     // TODO uncomment on Chrysalis Pt1.
        //     block_on(Protocol::broadcast_heartbeat(
        //         tangle().get_latest_solid_milestone_index(),
        //         tangle().get_pruning_index(),
        //     ));
        // });

        let milestone_solidifier = bee_node.worker::<MilestoneSolidifierWorker>().unwrap().tx.clone();
        let milestone_requester = bee_node.worker::<MilestoneRequesterWorker>().unwrap().tx.clone();

        bus.add_listener(move |latest_solid_milestone: &LatestSolidMilestoneChanged| {
            debug!("New solid milestone {}.", *latest_solid_milestone.0.index);
            tangle().update_latest_solid_milestone_index(latest_solid_milestone.0.index);

            let ms_sync_count = Protocol::get().config.workers.ms_sync_count;
            let next_ms = latest_solid_milestone.0.index + MilestoneIndex(ms_sync_count);

            if !tangle().is_synced() {
                if tangle().contains_milestone(next_ms) {
                    milestone_solidifier.send(MilestoneSolidifierWorkerEvent(next_ms));
                } else {
                    Protocol::request_milestone(&milestone_requester, next_ms, None);
                }
            }

            // TODO spawn ?
            Protocol::broadcast_heartbeat(
                latest_solid_milestone.0.index,
                tangle().get_pruning_index(),
                tangle().get_latest_milestone_index(),
            );
        });
    }

    pub(crate) fn get() -> &'static Protocol {
        if unsafe { PROTOCOL.is_null() } {
            panic!("Uninitialized protocol.");
        } else {
            unsafe { &*PROTOCOL }
        }
    }

    pub fn register(
        bee_node: &BeeNode,
        epid: EndpointId,
        address: SocketAddr,
        origin: Origin,
    ) -> (flume::Sender<Vec<u8>>, oneshot::Sender<()>) {
        // TODO check if not already added ?

        let peer = Arc::new(Peer::new(epid, address, origin));

        let (receiver_tx, receiver_rx) = flume::unbounded();
        let (receiver_shutdown_tx, receiver_shutdown_rx) = oneshot::channel();

        Protocol::get().peer_manager.add(peer.clone());

        spawn(
            PeerHandshakerWorker::new(
                Protocol::get().network.clone(),
                peer,
                bee_node.worker::<HasherWorker>().unwrap().tx.clone(),
                bee_node.worker::<TransactionResponderWorker>().unwrap().tx.clone(),
                bee_node.worker::<MilestoneResponderWorker>().unwrap().tx.clone(),
                bee_node.worker::<MilestoneRequesterWorker>().unwrap().tx.clone(),
            )
            .run(receiver_rx, receiver_shutdown_rx),
        );

        (receiver_tx, receiver_shutdown_tx)
    }
}
