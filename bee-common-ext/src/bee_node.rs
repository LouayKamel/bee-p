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

use crate::{node::Node, worker::Worker};

use anymap::{any::Any, Map};
use futures::{channel::oneshot, future::Future};
use tokio::spawn;

use std::{
    any::TypeId,
    collections::hash_map::{Entry, HashMap},
    sync::Mutex,
};

#[allow(clippy::type_complexity)]
pub struct BeeNode {
    workers: Map<dyn Any + Send + Sync>,
    tasks: Mutex<
        HashMap<
            TypeId,
            Vec<(
                oneshot::Sender<()>,
                // TODO Result ?
                Box<dyn Future<Output = Result<(), tokio::task::JoinError>> + Send + Sync>,
            )>,
        >,
    >,
}

impl Default for BeeNode {
    fn default() -> Self {
        Self {
            workers: Map::new(),
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

impl Node for BeeNode {
    fn new() -> Self {
        Self::default()
    }

    fn spawn<W, G, F>(&self, g: G)
    where
        Self: Sized,
        W: Worker<Self>,
        G: FnOnce(oneshot::Receiver<()>) -> F,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();

        if let Ok(mut tasks) = self.tasks.lock() {
            match tasks.entry(TypeId::of::<W>()) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().push((tx, Box::new(spawn(g(rx)))));
                }
                Entry::Vacant(entry) => {
                    entry.insert(vec![(tx, Box::new(spawn(g(rx))))]);
                }
            }
        }
    }

    fn worker<W>(&self) -> Option<&W>
    where
        Self: Sized,
        W: Worker<Self> + Send + Sync,
    {
        self.workers.get::<W>()
    }

    fn add_worker<W>(&mut self, worker: W)
    where
        Self: Sized,
        W: Worker<Self> + Send + Sync,
    {
        self.workers.insert(worker);
    }
}