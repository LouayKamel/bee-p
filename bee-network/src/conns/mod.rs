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

mod connection;
mod dial;
mod manager;

use crate::{
    interaction::events::{Event, EventSender},
    peers::{self, DataReceiver},
    MAX_BUFFER_SIZE,
};

pub use connection::Origin;
pub use dial::dial_peer;
pub use manager::ConnectionManager;

use connection::MuxedConnection;

use futures::{prelude::*, select, AsyncRead, AsyncWrite};
use libp2p::{
    core::muxing::{event_from_ref_and_wrap, outbound_from_ref_and_wrap, StreamMuxerBox, SubstreamRef},
    Multiaddr, PeerId,
};
use log::*;
use thiserror::Error as ErrorAttr;
use tokio::task::JoinHandle;

use std::sync::{atomic::Ordering, Arc};

pub(crate) async fn spawn_connection_handler(
    connection: MuxedConnection,
    internal_event_sender: EventSender,
) -> Result<(), Error> {
    let MuxedConnection {
        peer_id,
        peer_address,
        muxer,
        origin,
        ..
    } = connection;

    let muxer = Arc::new(muxer);
    let (data_sender, data_receiver) = peers::channel();

    let peer_id_clone = peer_id.clone();
    let peer_address_clone = peer_address.clone();
    let internal_event_sender_clone = internal_event_sender.clone();

    let substream = match origin {
        Origin::Outbound => outbound_from_ref_and_wrap(muxer).fuse().await.unwrap(),
        Origin::Inbound => loop {
            if let Some(substream) = event_from_ref_and_wrap(muxer.clone())
                .await
                .expect("error awaiting inbound substream")
                .into_inbound_substream()
            {
                break substream;
            }
        },
    };

    spawn_substream_task(
        peer_id_clone,
        peer_address_clone,
        substream,
        data_receiver,
        internal_event_sender_clone,
    );

    internal_event_sender
        .send_async(Event::ConnectionEstablished {
            peer_id,
            peer_address,
            origin,
            data_sender,
        })
        .await
        .map_err(|_| Error::SendingEventFailed)?;

    Ok(())
}

fn spawn_substream_task(
    peer_id: PeerId,
    peer_address: Multiaddr,
    mut substream: SubstreamRef<Arc<StreamMuxerBox>>,
    data_receiver: DataReceiver,
    mut internal_event_sender: EventSender,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut fused_data_receiver = data_receiver.into_stream();
        let mut buffer = vec![0u8; MAX_BUFFER_SIZE.load(Ordering::Relaxed)];

        loop {
            select! {
                num_read = recv_message(&mut substream, &mut buffer).fuse() => {
                    if !process_read(
                        peer_id.clone(),
                        peer_address.clone(),
                        num_read,
                        &mut internal_event_sender,
                        &buffer)
                        .await
                    {
                        break;
                    }
                }
                message = fused_data_receiver.next() => {
                    if let Some(message) = message {
                        send_message(&mut substream, &message).await;
                    } else {
                        // Data receiver closed => shutdown this task
                        break;
                    }

                }
            }
        }
    })
}

#[inline]
async fn send_message<S>(stream: &mut S, message: &[u8])
where
    S: AsyncWrite + Unpin,
{
    stream.write_all(message).await.expect("error write_all");
    stream.flush().await.expect("error flush");
    trace!("wrote {} bytes", message.len());
}

#[inline]
async fn recv_message<S>(stream: &mut S, message: &mut [u8]) -> usize
where
    S: AsyncRead + Unpin,
{
    let num_read = stream.read(message).await.expect("error read");
    trace!("read {} bytes", num_read);
    num_read
}

#[inline]
async fn process_read(
    peer_id: PeerId,
    peer_address: Multiaddr,
    num_read: usize,
    internal_event_sender: &mut EventSender,
    buffer: &[u8],
) -> bool {
    if num_read == 0 {
        trace!("Stream dropped by peer (EOF).");

        if internal_event_sender
            .send_async(Event::ConnectionDropped {
                peer_id: peer_id.clone(),
                peer_address: peer_address.clone(),
            })
            .await
            .is_err()
        {
            warn!("Dropped internal event (OOM?)");
        }

        false
    } else {
        let mut message = vec![0u8; num_read];
        message.copy_from_slice(&buffer[0..num_read]);

        if internal_event_sender
            .send_async(Event::MessageReceived {
                peer_id: peer_id.clone(),
                message,
            })
            .await
            .is_err()
        {
            warn!("Dropped internal event (OOM?)");
        }

        true
    }
}

#[derive(Debug, ErrorAttr)]
pub enum Error {
    #[error("An async I/O error occured.")]
    IoError(#[from] std::io::Error),

    #[error("Connection attempt failed.")]
    ConnectionAttemptFailed,

    #[error("Sending an event failed.")]
    SendingEventFailed,
}