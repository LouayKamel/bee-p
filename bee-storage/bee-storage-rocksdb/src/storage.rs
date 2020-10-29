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

use super::config::*;

pub use bee_storage::storage::Backend;

use async_trait::async_trait;
use blake2::{Blake2b, Digest};
use rocksdb::{ColumnFamilyDescriptor, DBCompactionStyle, DBCompressionType, Options, SliceTransform, DB};

use std::error::Error;

pub(crate) const MESSAGE_ID_TO_MESSAGE: &str = "message_id_to_message";
pub(crate) const PAYLOAD_INDEX_TO_MESSAGE_ID: &str = "payload_index_to_message_id";

pub struct Storage {
    pub(crate) inner: DB,
}

impl Storage {
    pub fn try_new(config: RocksDBConfig) -> Result<DB, Box<dyn Error>> {
        let message_id_to_message = ColumnFamilyDescriptor::new(MESSAGE_ID_TO_MESSAGE, Options::default());
        let prefix_extractor = SliceTransform::create_fixed_prefix(<Blake2b as Digest>::output_size());
        let mut options = Options::default();
        options.set_prefix_extractor(prefix_extractor);
        let payload_index_to_message_id = ColumnFamilyDescriptor::new(PAYLOAD_INDEX_TO_MESSAGE_ID, options.clone());

        let mut opts = Options::default();

        opts.create_if_missing(config.create_if_missing);
        opts.create_missing_column_families(config.create_missing_column_families);
        if config.enable_statistics {
            opts.enable_statistics();
        }
        opts.increase_parallelism(config.increase_parallelism);
        opts.optimize_for_point_lookup(config.optimize_for_point_lookup);
        opts.optimize_level_style_compaction(config.optimize_level_style_compaction);
        opts.optimize_universal_style_compaction(config.optimize_universal_style_compaction);
        opts.set_advise_random_on_open(config.set_advise_random_on_open);
        opts.set_allow_concurrent_memtable_write(config.set_allow_concurrent_memtable_write);
        opts.set_allow_mmap_reads(config.set_allow_mmap_reads);
        opts.set_allow_mmap_writes(config.set_allow_mmap_writes);
        opts.set_atomic_flush(config.set_atomic_flush);
        opts.set_bytes_per_sync(config.set_bytes_per_sync);
        opts.set_compaction_readahead_size(config.set_compaction_readahead_size);
        opts.set_compaction_style(DBCompactionStyle::from(config.set_compaction_style));
        opts.set_max_write_buffer_number(config.set_max_write_buffer_number);
        opts.set_disable_auto_compactions(config.set_disable_auto_compactions);
        opts.set_compression_type(DBCompressionType::from(config.set_compression_type));

        let column_familes = vec![message_id_to_message, payload_index_to_message_id];

        Ok(DB::open_cf_descriptors(&opts, config.path, column_familes)?)
    }
}

#[async_trait]
impl Backend for Storage {
    type ConfigBuilder = RocksDBConfigBuilder;
    type Config = RocksDBConfig;

    /// It starts RocksDB instance and then initializes the required column familes.
    async fn start(config: Self::Config) -> Result<Self, Box<dyn Error>> {
        Ok(Storage {
            inner: Self::try_new(config)?,
        })
    }

    /// It shutdown RocksDB instance.
    /// Note: the shutdown is done through flush method and then droping the storage object.
    async fn shutdown(self) -> Result<(), Box<dyn Error>> {
        if let Err(e) = self.inner.flush() {
            return Err(Box::new(e));
        }
        Ok(())
    }
}
