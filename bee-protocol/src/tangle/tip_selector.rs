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

use crate::tangle::tangle;
use bee_crypto::ternary::Hash;
use bee_transaction::Vertex;
use dashmap::{DashSet, DashMap};
use rand::{seq::IteratorRandom, thread_rng};
use std::collections::HashSet;
use bee_tangle::Tangle;
use dashmap::mapref::entry::Entry;
use std::sync::{Arc, RwLock};

enum Score {
    NON_LAZY,
    SEMI_LAZY,
    LAZY,
}

const C1: u32 = 8;
const C2: u32 = 13;
const M: u32 = 15;

const MAX_NUM_CHILDREN: u8 = 2;
const MAX_NUM_SELECTIONS: u8 = 2;

pub struct TipSelector {
    children: DashMap<Hash, HashSet<Hash>>,
    non_lazy_tips: DashSet<Hash>,
    semi_lazy_tips: DashSet<Hash>,
    update_score_lock: Arc<RwLock<()>>,
}

impl TipSelector {
    pub(crate) fn new() -> Self {
        Self {
            children: DashMap::new(),
            non_lazy_tips: DashSet::new(),
            semi_lazy_tips: DashSet::new(),
            update_score_lock: Arc::new(RwLock::new(())),
        }
    }

    // WURTS does only support solid transactions. This function therefore expects that each passed hash is solid.
    // In context of WURTS, a transaction can be considered as a "tip" if:
    // - transaction is solid and has no children
    // - transaction is solid and has only non-solid children
    // - transaction is solid and has solid children but does not exceed the retention rules
    pub fn insert(&self, hash: &Hash) {
        // Link parents with child
        self.add_to_parents(hash);
        // Remove parents that have more than 'MAX_CHILDREN_COUNT' children
        self.check_num_children_of_parents(hash);
    }

    fn add_to_parents(&self, hash: &Hash) {
        let (trunk, branch) = self.parents(hash);
        self.add_child(trunk, *hash);
        self.add_child(branch, *hash);
    }

    fn parents(&self, hash: &Hash) -> (Hash, Hash) {
        let tx = tangle().get(hash).unwrap();
        let trunk = tx.trunk();
        let branch = tx.branch();
        (*trunk, *branch)
    }

    fn add_child(&self, parent: Hash, child: Hash) {
        match self.children.entry(parent) {
            Entry::Occupied(mut entry) => {
                let children = entry.get_mut();
                children.insert(child);
            }
            Entry::Vacant(entry) => {
                let mut children = HashSet::new();
                children.insert(child);
                entry.insert(children);
            }
        }
    }

    fn check_num_children_of_parents(&self, hash: &Hash) {
        let (trunk, branch) = self.parents(hash);
        self.check_num_children_of_parent(&trunk);
        self.check_num_children_of_parent(&branch);
    }

    fn check_num_children_of_parent(&self, hash: &Hash) {
        if self.children.get(hash).unwrap().len() as u8 > MAX_NUM_CHILDREN {
            self.children.remove(&hash);
            self.non_lazy_tips.remove(&hash);
            self.semi_lazy_tips.remove(&hash);
        }
    }

    // further optimization: avoid allocations
    pub fn update_scores(&self) {

        // make sure tip selection is not performed while scores get updated
        self.update_score_lock.write().unwrap();

        // reset pools
        self.non_lazy_tips.clear();
        self.semi_lazy_tips.clear();

        // iter tips and assign them to their appropriate pools
        for (tip, _) in self.children.clone() {
            match self.tip_score(&tip) {
                Score::NON_LAZY => {
                    self.non_lazy_tips.insert(tip);
                }
                Score::SEMI_LAZY => {
                    self.semi_lazy_tips.insert(tip);
                }
                Score::LAZY => {
                    self.children.remove(&tip);
                }
            }
        }

    }

    fn tip_score(&self, hash: &Hash) -> Score {
        let lsmi = *tangle().get_last_solid_milestone_index();
        let otrsi = *tangle().otrsi(&hash).unwrap();
        let ytrsi = *tangle().ytrsi(&hash).unwrap();

        if (lsmi - ytrsi) > C1 {
            return Score::LAZY;
        }

        if (lsmi - otrsi) > C2 {
            return Score::LAZY;
        }

        if (lsmi - otrsi) > M {
            return Score::SEMI_LAZY;
        }

        Score::NON_LAZY
    }

    pub fn get_non_lazy_tips(&self) -> Option<(Hash, Hash)> {
        self.select_tips(&self.non_lazy_tips)
    }

    pub fn get_semi_lazy_tips(&self) -> Option<(Hash, Hash)> {
        self.select_tips(&self.semi_lazy_tips)
    }

    fn select_tips(&self, hashes: &DashSet<Hash>) -> Option<(Hash, Hash)> {

        self.update_score_lock.read().unwrap();

        self.check_num_selections();
        let mut ret = HashSet::new();
        // try to get 10x randomly a tip
        for i in 1..10 {
            match self.select_tip(hashes) {
                Some(tip) => {
                    ret.insert(tip);
                }
                None => (),
            }
        }
        if ret.is_empty() {
            return None;
        } else if ret.len() == 1 {
            let tip = ret.iter().next().unwrap();
            tangle().update_metadata(&tip, |metadata| {
                metadata.num_selected += 1;
            });
            return Some((*tip, *tip));
        } else {
            let mut iter = ret.iter();
            let tip_1 = *iter.next().unwrap();
            let tip_2 = *iter.next().unwrap();
            tangle().update_metadata(&tip_1, |metadata| {
                metadata.num_selected += 1;
            });
            tangle().update_metadata(&tip_2, |metadata| {
                metadata.num_selected += 1;
            });
            return Some((tip_1, tip_2));
        }
    }

    fn select_tip(&self, hashes: &DashSet<Hash>) -> Option<Hash> {
        if hashes.is_empty() {
            return None;
        }
        Some(*hashes.iter().choose(&mut rand::thread_rng()).unwrap())
    }

    fn check_num_selections(&self) {
        for (hash, _) in self.children.clone() {
            if tangle().is_solid_entry_point(&hash) {
                continue;
            } else {
                if tangle().get_metadata(&hash).unwrap().num_selected >= MAX_NUM_SELECTIONS {
                    self.children.remove(&hash);
                    self.non_lazy_tips.remove(&hash);
                    self.semi_lazy_tips.remove(&hash);
                }
            }
        }
    }

}
