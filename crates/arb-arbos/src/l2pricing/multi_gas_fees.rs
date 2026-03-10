// // Copyright 2025-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package l2pricing

// import (
// 	"math/big"

// 	"github.com/ethereum/go-ethereum/arbitrum/multigas"

// 	"github.com/offchainlabs/nitro/arbos/storage"
// )

// const (
// 	nextBlockFeesOffset uint64 = iota * uint64(multigas.NumResourceKind)
// 	currentBlockFeesOffset
// )

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::I256,
};
use arbitrum::multigas::resources::ResourceKind;
use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedBigInt},
};

// nextBlockFeesOffset  = 0 * NumResourceKind = 0
// currentBlockFeesOffset = 1 * NumResourceKind = 8
const NEXT_BLOCK_FEES_OFFSET: u64 = 0;
const CURRENT_BLOCK_FEES_OFFSET: u64 = ResourceKind::COUNT as u64;

// // MultiGasFees tracks per–resource-kind base fees.
// // The `next` field is the base fee for future blocks. It is updated alongside l2pricing.baseFee whenever `updateMultiGasConstraintsBacklogs` is called.
// // The `current` field is the base-fee for the current block, and it is updated in `arbos.ProduceBlockAdvanced` before executing transactions.
// type MultiGasFees struct {
// 	next    [multigas.NumResourceKind]storage.StorageBackedBigInt
// 	current [multigas.NumResourceKind]storage.StorageBackedBigInt
// }

pub struct MultiGasFees<B: Burner> {
    next: [StorageBackedBigInt<B>; ResourceKind::COUNT],
    current: [StorageBackedBigInt<B>; ResourceKind::COUNT],
}

impl<B: Burner> MultiGasFees<B> {
    // func OpenMultiGasFees(sto *storage.Storage) *MultiGasFees
    pub fn open(sto: &Storage<B>) -> Self
    where
        B: Clone,
    {
        MultiGasFees {
            next: core::array::from_fn(|i| {
                sto.open_storage_backed_big_int(NEXT_BLOCK_FEES_OFFSET + i as u64)
            }),
            current: core::array::from_fn(|i| {
                sto.open_storage_backed_big_int(CURRENT_BLOCK_FEES_OFFSET + i as u64)
            }),
        }
    }

    // func (bf *MultiGasFees) GetCurrentBlockFee(kind multigas.ResourceKind) (*big.Int, error)
    pub fn get_current_block_fee<Db: Database>(
        &self,
        kind: ResourceKind,
        db: &mut Db,
    ) -> Result<I256, Db::Error> {
        self.current[kind as usize].get(db)
    }

    // func (bf *MultiGasFees) GetNextBlockFee(kind multigas.ResourceKind) (*big.Int, error)
    pub fn get_next_block_fee<Db: Database>(
        &self,
        kind: ResourceKind,
        db: &mut Db,
    ) -> Result<I256, Db::Error> {
        self.next[kind as usize].get(db)
    }

    // func (bf *MultiGasFees) SetNextBlockFee(kind multigas.ResourceKind, v *big.Int) error
    pub fn set_next_block_fee<CTX: ContextTr>(
        &mut self,
        kind: ResourceKind,
        v: I256,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.next[kind as usize].set_checked(ctx, v)
    }

    // func (bf *MultiGasFees) CommitNextToCurrent() error
    pub fn commit_next_to_current<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        for i in 0..ResourceKind::COUNT {
            // Go: if cur == nil { cur = big.NewInt(0) } — I256::ZERO is the unset default in Rust.
            let val = self.next[i].get(ctx.db_mut())?;
            self.current[i].set_checked(ctx, val)?;
        }
        Ok(())
    }
}
