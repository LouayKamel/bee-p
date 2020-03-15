use crate::message::{
    Handshake, Header, Heartbeat, LegacyGossip, Message, MilestoneRequest, TransactionBroadcast, TransactionRequest,
};
use crate::neighbor::SenderWorker;
use crate::protocol::{COORDINATOR_BYTES, MINIMUM_WEIGHT_MAGNITUDE, SUPPORTED_VERSIONS};

use netzwerk::Command::SendBytes;
use netzwerk::{Network, PeerId};

use std::convert::TryInto;

use async_std::task::spawn;
use futures::channel::mpsc::Receiver;
use futures::stream::StreamExt;
use log::*;

pub enum ReceiverWorkerEvent {
    Removed,
    Connected,
    Disconnected,
    Message { size: usize, bytes: Vec<u8> },
}

enum ReceiverWorkerMessageState {
    Header,
    Payload(Header),
}

struct AwaitingConnectionContext {}

struct AwaitingHandshakeContext {
    state: ReceiverWorkerMessageState,
}

struct AwaitingMessageContext {
    state: ReceiverWorkerMessageState,
}

enum ReceiverWorkerState {
    AwaitingConnection(AwaitingConnectionContext),
    AwaitingHandshake(AwaitingHandshakeContext),
    AwaitingMessage(AwaitingMessageContext),
}

pub(crate) struct ReceiverWorker {
    peer_id: PeerId,
    network: Network,
    receiver: Receiver<ReceiverWorkerEvent>,
}

impl ReceiverWorker {
    pub(crate) fn new(peer_id: PeerId, network: Network, receiver: Receiver<ReceiverWorkerEvent>) -> Self {
        Self {
            peer_id: peer_id,
            network: network,
            receiver: receiver,
        }
    }

    async fn send_handshake(&mut self) {
        // TODO metric ?
        // TODO port
        let bytes =
            Handshake::new(1337, &COORDINATOR_BYTES, MINIMUM_WEIGHT_MAGNITUDE, &SUPPORTED_VERSIONS).into_full_bytes();

        self.network
            .send(SendBytes {
                to_peer: self.peer_id,
                bytes: bytes.to_vec(),
            })
            .await;
    }

    pub(crate) async fn run(mut self) {
        let mut state = ReceiverWorkerState::AwaitingConnection(AwaitingConnectionContext {});

        while let Some(event) = self.receiver.next().await {
            if let ReceiverWorkerEvent::Removed = event {
                break;
            }

            state = match state {
                ReceiverWorkerState::AwaitingConnection(context) => self.connection_handler(context, event).await,
                ReceiverWorkerState::AwaitingHandshake(context) => self.handshake_handler(context, event).await,
                ReceiverWorkerState::AwaitingMessage(context) => self.message_handler(context, event).await,
            }
        }
    }

    async fn connection_handler(
        &mut self,
        context: AwaitingConnectionContext,
        event: ReceiverWorkerEvent,
    ) -> ReceiverWorkerState {
        match event {
            ReceiverWorkerEvent::Connected => {
                info!("[Neighbor-{:?}] Connected", self.peer_id);

                // TODO spawn ?
                self.send_handshake().await;

                ReceiverWorkerState::AwaitingHandshake(AwaitingHandshakeContext {
                    state: ReceiverWorkerMessageState::Header,
                })
            }
            _ => ReceiverWorkerState::AwaitingConnection(context),
        }
    }

    fn check_handshake(&self, header: Header, bytes: &[u8]) -> ReceiverWorkerState {
        info!("[Neighbor-{:?}] Reading Handshake", self.peer_id);

        match Handshake::from_full_bytes(&header, bytes) {
            Ok(_) => {
                // TODO check handshake

                ReceiverWorkerState::AwaitingMessage(AwaitingMessageContext {
                    state: ReceiverWorkerMessageState::Header,
                })
            }

            Err(e) => {
                warn!("[Neighbor-{:?}] Reading Handshake failed: {:?}", self.peer_id, e);

                ReceiverWorkerState::AwaitingHandshake(AwaitingHandshakeContext {
                    state: ReceiverWorkerMessageState::Header,
                })
            }
        }
    }

    async fn handshake_handler(
        &mut self,
        context: AwaitingHandshakeContext,
        event: ReceiverWorkerEvent,
    ) -> ReceiverWorkerState {
        match event {
            ReceiverWorkerEvent::Disconnected => {
                info!("[Neighbor-{:?}] Disconnected", self.peer_id);

                ReceiverWorkerState::AwaitingConnection(AwaitingConnectionContext {})
            }
            ReceiverWorkerEvent::Message { size, bytes } => {
                info!("[Neighbor-{:?}] Message received", self.peer_id);

                if size < 3 {
                    ReceiverWorkerState::AwaitingHandshake(AwaitingHandshakeContext {
                        state: ReceiverWorkerMessageState::Header,
                    })
                } else {
                    match context.state {
                        ReceiverWorkerMessageState::Header => {
                            info!("[Neighbor-{:?}] Reading Header", self.peer_id);

                            let header = bytes[0..3].try_into().unwrap();

                            if size > 3 {
                                self.check_handshake(header, &bytes[3..size])
                            } else {
                                ReceiverWorkerState::AwaitingHandshake(AwaitingHandshakeContext {
                                    state: ReceiverWorkerMessageState::Payload(header),
                                })
                            }
                        }
                        ReceiverWorkerMessageState::Payload(header) => self.check_handshake(header, &bytes[..size]),
                    }
                }
            }
            _ => ReceiverWorkerState::AwaitingHandshake(context),
        }
    }

    fn process_message(&self, header: Header, bytes: &[u8]) -> ReceiverWorkerState {
        // TODO metrics
        match header[0] {
            Handshake::ID => {
                warn!("[Neighbor-{:?}] Reading unexpected Handshake", self.peer_id);

                match Handshake::from_full_bytes(&header, bytes) {
                    Ok(_) => {
                        // Not expecting a handshake at this point, ignore
                    }
                    Err(e) => {
                        warn!(
                            "[Neighbor-{:?}] Reading unexpected Handshake failed: {:?}",
                            self.peer_id, e
                        );
                    }
                }
            }

            LegacyGossip::ID => {
                info!("[Neighbor-{:?}] Reading LegacyGossip", self.peer_id);

                match LegacyGossip::from_full_bytes(&header, bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("[Neighbor-{:?}] Reading LegacyGossip failed: {:?}", self.peer_id, e);
                    }
                }
            }

            MilestoneRequest::ID => {
                info!("[Neighbor-{:?}] Reading MilestoneRequest", self.peer_id);

                match MilestoneRequest::from_full_bytes(&header, bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("[Neighbor-{:?}] Reading MilestoneRequest failed: {:?}", self.peer_id, e);
                    }
                }
            }

            TransactionBroadcast::ID => {
                info!("[Neighbor-{:?}] Reading TransactionBroadcast", self.peer_id);

                match TransactionBroadcast::from_full_bytes(&header, bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(
                            "[Neighbor-{:?}] Reading TransactionBroadcast failed: {:?}",
                            self.peer_id, e
                        );
                    }
                }
            }

            TransactionRequest::ID => {
                info!("[Neighbor-{:?}] Reading TransactionRequest", self.peer_id);

                match TransactionRequest::from_full_bytes(&header, bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(
                            "[Neighbor-{:?}] Reading TransactionRequest failed: {:?}",
                            self.peer_id, e
                        );
                    }
                }
            }

            Heartbeat::ID => {
                info!("[Neighbor-{:?}] Reading Heartbeat", self.peer_id);

                match Heartbeat::from_full_bytes(&header, bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("[Neighbor-{:?}] Reading Heartbeat failed: {:?}", self.peer_id, e);
                    }
                }
            }

            _ => {
                // _ => Err(MessageError::InvalidMessageType(message_type)),
            }
        }

        ReceiverWorkerState::AwaitingMessage(AwaitingMessageContext {
            state: ReceiverWorkerMessageState::Header,
        })
    }

    async fn message_handler(
        &mut self,
        context: AwaitingMessageContext,
        event: ReceiverWorkerEvent,
    ) -> ReceiverWorkerState {
        // spawn(SenderWorker::<LegacyGossip>::new(self.peer_id, self.network.clone()).run());
        // spawn(SenderWorker::<MilestoneRequest>::new(self.peer_id, self.network.clone()).run());
        // spawn(SenderWorker::<TransactionBroadcast>::new(self.peer_id, self.network.clone()).run());
        // spawn(SenderWorker::<TransactionRequest>::new(self.peer_id, self.network.clone()).run());
        // spawn(SenderWorker::<Heartbeat>::new(self.peer_id, self.network.clone()).run());

        match event {
            ReceiverWorkerEvent::Disconnected => {
                info!("[Neighbor-{:?}] Disconnected", self.peer_id);

                ReceiverWorkerState::AwaitingConnection(AwaitingConnectionContext {})
            }
            ReceiverWorkerEvent::Message { size, bytes } => {
                info!("[Neighbor-{:?}] Message received", self.peer_id);

                if size < 3 {
                    ReceiverWorkerState::AwaitingMessage(AwaitingMessageContext {
                        state: ReceiverWorkerMessageState::Header,
                    })
                } else {
                    match context.state {
                        ReceiverWorkerMessageState::Header => {
                            info!("[Neighbor-{:?}] Reading Header", self.peer_id);

                            let header = bytes[0..3].try_into().unwrap();

                            if size > 3 {
                                self.process_message(header, &bytes[3..size])
                            } else {
                                ReceiverWorkerState::AwaitingMessage(AwaitingMessageContext {
                                    state: ReceiverWorkerMessageState::Payload(header),
                                })
                            }
                        }
                        ReceiverWorkerMessageState::Payload(header) => self.process_message(header, &bytes[..size]),
                    }
                }
            }
            _ => ReceiverWorkerState::AwaitingMessage(context),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
}
