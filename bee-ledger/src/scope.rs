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

use bee_storage::access::*;
use blake2::Blake2b;

use crate::{output::Output, spent::Spent};
use bee_message::{
    payload::{indexation::HashedIndex, transaction::OutputId},
    Message, MessageId,
};

/// NodeScope
pub trait LedgerScope:
    Fetch<MessageId, Message>
    + Fetch<MessageId, Vec<MessageId>>
    + Fetch<HashedIndex<Blake2b>, Vec<MessageId>>
    + Fetch<OutputId, Output>
    + Fetch<OutputId, Spent>
    + Insert<MessageId, Message>
    + Insert<(MessageId, MessageId), ()>
    + Insert<(HashedIndex<Blake2b>, MessageId), ()>
    + Insert<OutputId, Output>
    + Insert<OutputId, Spent>
    + Delete<MessageId, Message>
    + Delete<(MessageId, MessageId), ()>
    + Delete<(HashedIndex<Blake2b>, MessageId), ()>
    + Delete<OutputId, Output>
    + Delete<OutputId, Spent>
    + Exist<MessageId, Message>
    + Exist<MessageId, Vec<MessageId>>
    + Exist<HashedIndex<Blake2b>, Vec<MessageId>>
    + Exist<OutputId, Output>
    + Exist<OutputId, Spent> // todo add batch operations bounds
{
}
