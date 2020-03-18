use crate::constants::BYTES_CHANNEL_CAPACITY;
use crate::endpoint::EndpointId as EpId;
use crate::errors::ActorResult as R;

use async_std::sync::Arc;
use futures::channel::mpsc;
use futures::sink::SinkExt;

use std::collections::hash_map::Entry;
use std::collections::HashMap;

pub type BytesSender = mpsc::Sender<Arc<Vec<u8>>>;
pub type BytesReceiver = mpsc::Receiver<Arc<Vec<u8>>>;

pub fn bytes_channel() -> (BytesSender, BytesReceiver) {
    mpsc::channel(BYTES_CHANNEL_CAPACITY)
}

pub struct Outbox {
    inner: HashMap<EpId, BytesSender>,
}

impl Outbox {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    /// Returns the size of the pool.
    pub fn size(&self) -> usize {
        self.inner.len()
    }

    /// Inserts a `sender` to the pool.
    ///
    /// NOTE: Inserts only if there is no entry with the endpoint id yet.
    pub fn insert(&mut self, epid: EpId, sender: BytesSender) -> bool {
        match self.inner.entry(epid.clone()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(sender);
                true
            }
        }
    }

    /// Removes a `sender` associated with an endpoint.
    pub fn remove(&mut self, id: &EpId) -> bool {
        self.inner.remove(id).is_some()
    }

    /// Checks whether the specified endpoint belongs to the pool.
    pub fn contains(&self, id: &EpId) -> bool {
        self.inner.contains_key(id)
    }

    /// Sends `bytes` to `receiver`.
    ///
    /// Returns `true` if the send was successful.
    pub async fn send(&mut self, bytes: Vec<u8>, recipient: &EpId) -> R<bool> {
        let bytes = Arc::new(bytes);
        if let Some(sender) = self.inner.get_mut(recipient) {
            sender.send(bytes).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Multicasts `bytes` to the `receivers`.
    ///
    /// NOTE: The multicast is considered to be successful, if at least
    /// one send is successful.
    pub async fn multicast(&mut self, bytes: Vec<u8>, recipients: &Vec<EpId>) -> R<bool> {
        let bytes = Arc::new(bytes);
        let mut num_sends = 0;

        // TODO: Do not block!
        for (epid, sender) in self.inner.iter_mut() {
            if recipients.contains(epid) {
                sender.send(Arc::clone(&bytes)).await?;
                num_sends += 1;
            }
        }

        Ok(num_sends > 0)
    }

    /// Broadcasts `bytes` using all available connections from the pool.
    ///
    /// NOTE: The broadcast is considered to be successful, if at least
    /// one send is successful.
    pub async fn broadcast(&mut self, bytes: Vec<u8>) -> R<bool> {
        let bytes = Arc::new(bytes);
        let mut num_sends = 0;

        // TODO: Do not block!
        for (_, sender) in self.inner.iter_mut() {
            sender.send(Arc::clone(&bytes)).await?;
            num_sends += 1;
        }

        Ok(num_sends > 0)
    }
}