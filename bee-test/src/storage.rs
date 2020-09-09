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
#[allow(dead_code)]
async fn start_and_shutdown_rocksdb_storage() {
    // import storage
    use bee_storage_rocksdb::storage::{Backend, Storage};
    // start storage
    let storage: Storage = Storage::start("../bee-storage-rocksdb/config.toml".to_string())
        .await
        .unwrap();
    // shutdown storage
    assert!(storage.shutdown().await.is_ok())
}
#[allow(dead_code)]
async fn persist_ledger_diff() {
    // imports
    use bee_ledger::diff::*;
    use bee_protocol::MilestoneIndex;
    use bee_storage::access::{Delete, Fetch, Insert};
    use bee_storage_rocksdb::storage::{Backend, Storage};
    // start storage
    let storage: Storage = Storage::start("../bee-storage-rocksdb/config.toml".to_string())
        .await
        .unwrap();
    // create empty ledger_diff
    let ledger_diff: LedgerDiff = LedgerDiff::new();
    // milestone_index
    let ms = MilestoneIndex(0);
    // persist it
    assert!(storage.insert(&ms, &ledger_diff).await.is_ok());
    // find it
    if let Ok(same_ledger_diff) = storage.fetch(&ms).await {
        assert!(same_ledger_diff.is_some());
    } else {
        panic!("persist_ledger_diff test")
    };
    // delete
    assert!(storage.delete(&ms).await.is_ok());
    // shutdown storage
    assert!(storage.shutdown().await.is_ok())
}

use bee_transaction::bundled::{
        Address, BundledTransaction as Transaction, BundledTransactionBuilder as TransactionBuilder,
        BundledTransactionField, Index, Nonce, Payload, Tag, Timestamp, Value,
    };
    
#[allow(dead_code)]
async fn batch_storage() {
    // imports
    use bee_ledger::diff::*;
    use bee_protocol::MilestoneIndex;
    use bee_storage::access::{Delete, Fetch, Insert, Batch, BatchBuilder};
    use bee_storage_rocksdb::storage::{Backend, Storage};

    use crate::transaction::create_random_tx;
    use bee_crypto::ternary::Hash;
    use bee_transaction::bundled::BundledTransaction;
    // start storage
    let storage: Storage = Storage::start("../bee-storage-rocksdb/config.toml".to_string())
        .await
        .unwrap();
    // insert ledger_diff test
    let ledger_diff: LedgerDiff = LedgerDiff::new();
    let ms: MilestoneIndex = MilestoneIndex(0);
    storage.insert(&ms, &ledger_diff).await;
    // insert tx test
    let (hash, tx): (Hash, BundledTransaction) = create_random_tx();
    storage.insert(&hash, &tx).await; // this fail
    // persist it
    assert!(storage.insert(&ms, &ledger_diff).await.is_ok());
    // create empty ledger_diff
    let ledger_diff: LedgerDiff = LedgerDiff::new();
    // milestone_index
    let ms1 = MilestoneIndex(1);
    // persist it
    assert!(storage.insert(&ms1, &ledger_diff).await.is_ok());
    // find it
    if let Ok(same_ledger_diff) = storage.fetch(&ms).await {
        assert!(same_ledger_diff.is_some());
    } else {
        panic!("persist_ledger_diff test")
    };
    // find it
    if let Ok(same_ledger_diff) = storage.fetch(&ms1).await {
        assert!(same_ledger_diff.is_some());
    } else {
        panic!("persist_ledger_diff test")
    };
    // delete
    assert!(storage.delete(&ms).await.is_ok());
    // shutdown storage
    assert!(storage.shutdown().await.is_ok())
}

#[async_std::test]
async fn storage() {
    start_and_shutdown_rocksdb_storage().await;
    persist_ledger_diff().await;
}
