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

use bee_common::logger::logger_init;
use bee_node::{CliArgs, Node, NodeConfigBuilder};

const CONFIG_PATH: &str = "./config.toml";

fn main() {
    match NodeConfigBuilder::from_file(CONFIG_PATH) {
        Ok(mut config_builder) => {
            CliArgs::new().apply_to_config(&mut config_builder);
            let config = config_builder.finish();

            logger_init(config.logger.clone()).unwrap();
            // build tokio runtime
            let mut runtime = tokio::runtime::Builder::new()
                .threaded_scheduler()
                .core_threads(config.tokio.core_threads)
                .enable_io()
                .enable_time()
                .thread_name(config.tokio.thread_name.clone())
                .thread_stack_size(config.tokio.thread_stack_size)
                .build()
                .expect("Program aborted. Error was");
            // create
            runtime.block_on(async {
                match Node::build(config).finish().await {
                    Ok(mut node) => {
                        node.run_loop().await;
                        node.shutdown().await;
                    }
                    Err(e) => {
                        eprintln!("Program aborted. Error was: {}", e);
                    }
                }
            }
        );
        }
        Err(e) => {
            eprintln!("Program aborted. Error was: {}", e);
        }
    }
}
