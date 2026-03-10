// Copyright 2025-2026, Offchain Labs, Inc.
// For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

use std::collections::HashMap;
use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
};
use arbitrum::multigas::resources::ResourceKind;
use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedUint32, StorageBackedUint64},
};

// Fixed flat layout for a Multi-Constraint:
// [0] target (uint64)
// [1] adjustmentWindow (uint32)
// [2] backlog (uint64)
// [3] maxWeight (uint64)
// [4..4+NumResourceKind-1] weightedResources (uint64 each)
const TARGET_OFFSET: u64 = 0;
const ADJUSTMENT_WINDOW_OFFSET: u64 = 1;
const BACKLOG_OFFSET: u64 = 2;
const MAX_WEIGHT_OFFSET: u64 = 3;
const WEIGHTED_RESOURCES_BASE_OFFSET: u64 = 4;

// type MultiGasConstraint struct {
// 	target            storage.StorageBackedUint64
// 	adjustmentWindow  storage.StorageBackedUint32
// 	backlog           storage.StorageBackedUint64
// 	maxWeight         storage.StorageBackedUint64
// 	weightedResources [multigas.NumResourceKind]storage.StorageBackedUint64
// }
pub struct MultiGasConstraint<B: Burner> {
    target: StorageBackedUint64<B>,
    adjustment_window: StorageBackedUint32<B>,
    backlog: StorageBackedUint64<B>,
    max_weight: StorageBackedUint64<B>,
    weighted_resources: [StorageBackedUint64<B>; ResourceKind::COUNT],
}

impl<B: Burner> MultiGasConstraint<B> {
    // func OpenMultiGasConstraint(sto *storage.Storage) *MultiGasConstraint
    pub fn open(sto: &Storage<B>) -> Self
    where
        B: Clone,
    {
        MultiGasConstraint {
            target: sto.open_storage_backed_uint64(TARGET_OFFSET),
            adjustment_window: sto.open_storage_backed_uint32(ADJUSTMENT_WINDOW_OFFSET),
            backlog: sto.open_storage_backed_uint64(BACKLOG_OFFSET),
            max_weight: sto.open_storage_backed_uint64(MAX_WEIGHT_OFFSET),
            weighted_resources: core::array::from_fn(|i| {
                sto.open_storage_backed_uint64(WEIGHTED_RESOURCES_BASE_OFFSET + i as u64)
            }),
        }
    }

    // func (c *MultiGasConstraint) Clear() error
    pub fn clear<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.target.clear(ctx)?;
        self.adjustment_window.clear(ctx)?;
        self.backlog.clear(ctx)?;
        self.max_weight.clear(ctx)?;
        for i in 0..ResourceKind::COUNT {
            self.weighted_resources[i].clear(ctx)?;
        }
        Ok(())
    }

    // func (c *MultiGasConstraint) SetResourceWeights(weights map[uint8]uint64) error
    pub fn set_resource_weights<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        weights: &HashMap<u8, u64>,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let mut max_weight: u64 = 0;
        for i in 0..ResourceKind::COUNT {
            let weight = weights.get(&(i as u8)).copied().unwrap_or(0);
            if weight > max_weight {
                max_weight = weight;
            }
            self.weighted_resources[i].set(ctx, weight)?;
        }
        self.max_weight.set(ctx, max_weight)
    }

    // func (c *MultiGasConstraint) Target() (uint64, error)
    pub fn target<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.target.get(db)
    }

    // func (c *MultiGasConstraint) SetTarget(v uint64) error
    pub fn set_target<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.target.set(ctx, val)
    }

    // func (c *MultiGasConstraint) AdjustmentWindow() (uint32, error)
    pub fn adjustment_window<Db: Database>(&self, db: &mut Db) -> Result<u32, Db::Error> {
        self.adjustment_window.get(db)
    }

    // func (c *MultiGasConstraint) SetAdjustmentWindow(v uint32) error
    pub fn set_adjustment_window<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u32,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.adjustment_window.set(ctx, val)
    }

    // func (c *MultiGasConstraint) Backlog() (uint64, error)
    pub fn backlog<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.backlog.get(db)
    }

    // func (c *MultiGasConstraint) SetBacklog(v uint64) error
    pub fn set_backlog<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.backlog.set(ctx, val)
    }
}
