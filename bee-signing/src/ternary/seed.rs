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

use bee_common_derive::{SecretDebug, SecretDisplay, SecretDrop};
use bee_crypto_ext::ternary::sponge::Sponge;
use bee_ternary::{Btrit, Trit, TritBuf, Trits, T1B1};

use rand::Rng;
use thiserror::Error;
use zeroize::Zeroize;

use std::marker::PhantomData;

pub trait Seed: Zeroize + Drop {
    type Error;

    /// Creates a new `Seed`.
    fn new() -> Self;

    /// Creates a new `Seed` from the current `Seed` and an index.
    fn subseed(&self, index: u64) -> Self;

    /// Creates a `Seed` from trits.
    fn from_trits(buf: TritBuf) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Returns the inner trits.
    fn as_trits(&self) -> &Trits;
}

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Invalid seed length.")]
    InvalidLength(usize),
}

#[derive(SecretDebug, SecretDisplay, SecretDrop)]
pub struct TernarySeed<S> {
    seed: TritBuf,
    _sponge: PhantomData<S>,
}

impl<S> Zeroize for TernarySeed<S> {
    fn zeroize(&mut self) {
        unsafe { self.seed.as_i8_slice_mut().zeroize() }
    }
}

impl<S: Sponge + Default> Seed for TernarySeed<S> {
    type Error = Error;

    // TODO: is this random enough ?
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        // TODO out of here ?
        let trits = [-1, 0, 1];
        let seed: Vec<i8> = (0..243).map(|_| trits[rng.gen_range(0, trits.len())]).collect();

        Self {
            // Hello, future programmer! If you get a type error here, you're probably trying to
            // make this function generic over an encoding. Be aware that interpreting these raw i8
            // bytes as trits is a bad idea for encodings other than `T1B1`. In fact, that's why
            // I put this (currently unnecessary) type annotation here! To produce a warning that
            // hopefully means you read this text! If you still want to make this generic, the best
            // option is to just iterate through the `i8`s, convert them each to a trit, and then
            // collect them into a `TritBuf`
            seed: unsafe { Trits::<T1B1>::from_raw_unchecked(&seed, 243).to_buf() },
            _sponge: PhantomData,
        }
    }

    fn subseed(&self, index: u64) -> Self {
        let mut sponge = S::default();
        let mut subseed = self.seed.clone();

        for _ in 0..index {
            // TODO Put in trit utilities file
            for i in 0..subseed.len() {
                if let Some(ntrit) = subseed.get(i).unwrap().checked_increment() {
                    subseed.set(i, ntrit);
                    break;
                } else {
                    subseed.set(i, Btrit::NegOne);
                }
            }
        }

        // TODO return error
        let tmp = match sponge.digest(&subseed) {
            Ok(buf) => buf,
            Err(_) => unreachable!(),
        };

        Self {
            seed: tmp,
            _sponge: PhantomData,
        }
    }

    fn from_trits(buf: TritBuf) -> Result<Self, Self::Error> {
        if buf.len() != 243 {
            return Err(Self::Error::InvalidLength(buf.len()));
        }

        Ok(Self {
            seed: buf,
            _sponge: PhantomData,
        })
    }

    fn as_trits(&self) -> &Trits {
        &self.seed
    }
}