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

use crate::{access::OpError, storage::*};

use bee_common::packable::Packable;
use bee_ledger::spent::Spent;
use bee_message::{
    payload::{indexation::HashedIndex, transaction::OutputId},
    Message, MessageId, MESSAGE_ID_LENGTH,
};
use bee_storage::access::Fetch;

use blake2::{Blake2b, Digest};

use std::convert::TryInto;

#[async_trait::async_trait]
impl Fetch<MessageId, Message> for Storage {
    type Error = OpError;

    async fn fetch(&self, message_id: &MessageId) -> Result<Option<Message>, Self::Error>
    where
        Self: Sized,
    {
        let cf_message_id_to_message = self.inner.cf_handle(CF_MESSAGE_ID_TO_MESSAGE).unwrap();

        if let Some(res) = self.inner.get_cf(&cf_message_id_to_message, message_id)? {
            Ok(Some(Message::unpack(&mut res.as_slice()).unwrap()))
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl Fetch<MessageId, Vec<MessageId>> for Storage {
    type Error = OpError;

    async fn fetch(&self, parent: &MessageId) -> Result<Option<Vec<MessageId>>, Self::Error>
    where
        Self: Sized,
    {
        let cf_message_id_to_message_id = self.inner.cf_handle(CF_MESSAGE_ID_TO_MESSAGE_ID).unwrap();
        // TODO limit to a certain number of results

        Ok(Some(
            self.inner
                .prefix_iterator_cf(&cf_message_id_to_message_id, parent)
                .map(|(key, _)| {
                    let (_, child) = key.split_at(Blake2b::output_size());
                    let child: [u8; MESSAGE_ID_LENGTH] = child.try_into().unwrap();
                    MessageId::from(child)
                })
                .collect(),
        ))
    }
}

#[async_trait::async_trait]
impl Fetch<HashedIndex<Blake2b>, Vec<MessageId>> for Storage {
    type Error = OpError;

    async fn fetch(&self, index: &HashedIndex<Blake2b>) -> Result<Option<Vec<MessageId>>, Self::Error>
    where
        Self: Sized,
    {
        let cf_payload_index_to_message_id = self.inner.cf_handle(CF_PAYLOAD_INDEX_TO_MESSAGE_ID).unwrap();
        // TODO limit to a certain number of results

        Ok(Some(
            self.inner
                .prefix_iterator_cf(&cf_payload_index_to_message_id, index)
                .map(|(key, _)| {
                    let (_, message_id) = key.split_at(Blake2b::output_size());
                    let message_id: [u8; MESSAGE_ID_LENGTH] = message_id.try_into().unwrap();
                    MessageId::from(message_id)
                })
                .collect(),
        ))
    }
}

#[async_trait::async_trait]
impl Fetch<OutputId, Spent> for Storage {
    type Error = OpError;

    async fn fetch(&self, output_id: &OutputId) -> Result<Option<Spent>, Self::Error>
    where
        Self: Sized,
    {
        let cf_output_id_to_spent = self.inner.cf_handle(CF_OUTPUT_ID_TO_SPENT).unwrap();

        let mut output_id_buf = Vec::with_capacity(output_id.packed_len());
        // Packing to bytes can't fail.
        output_id.pack(&mut output_id_buf).unwrap();

        if let Some(res) = self.inner.get_cf(&cf_output_id_to_spent, output_id_buf)? {
            Ok(Some(Spent::unpack(&mut res.as_slice()).unwrap()))
        } else {
            Ok(None)
        }
    }
}
