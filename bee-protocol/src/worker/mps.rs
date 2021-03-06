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

use crate::{event::TpsMetricsUpdated, protocol::Protocol};

use bee_common::shutdown_stream::ShutdownStream;
use bee_common_ext::{event::Bus, node::Node, worker::Worker};

use async_trait::async_trait;
use futures::StreamExt;
use log::info;
use tokio::time::interval;

use std::{convert::Infallible, time::Duration};

const MPS_INTERVAL_SEC: u64 = 1;

#[derive(Default)]
pub(crate) struct MpsWorker {}

#[async_trait]
impl<N: Node> Worker<N> for MpsWorker {
    type Config = ();
    type Error = Infallible;

    async fn start(node: &mut N, _config: Self::Config) -> Result<Self, Self::Error> {
        let bus = node.resource::<Bus>();

        node.spawn::<Self, _, _>(|shutdown| async move {
            info!("Running.");

            let mut ticker = ShutdownStream::new(shutdown, interval(Duration::from_secs(MPS_INTERVAL_SEC)));

            let mut total_incoming = 0u64;
            let mut total_new = 0u64;
            let mut total_known = 0u64;
            let mut total_invalid = 0u64;
            let mut total_outgoing = 0u64;

            while ticker.next().await.is_some() {
                let incoming = Protocol::get().metrics.messages_received();
                let new = Protocol::get().metrics.new_messages();
                let known = Protocol::get().metrics.known_messages();
                let invalid = Protocol::get().metrics.invalid_messages();
                let outgoing = Protocol::get().metrics.messages_sent();

                bus.dispatch(TpsMetricsUpdated {
                    incoming: incoming - total_incoming,
                    new: new - total_new,
                    known: known - total_known,
                    invalid: invalid - total_invalid,
                    outgoing: outgoing - total_outgoing,
                });

                total_incoming = incoming;
                total_new = new;
                total_known = known;
                total_invalid = invalid;
                total_outgoing = outgoing;
            }

            info!("Stopped.");
        });

        Ok(Self::default())
    }
}
