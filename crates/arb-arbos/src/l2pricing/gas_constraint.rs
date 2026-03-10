// Copyright 2025-2026, Offchain Labs, Inc.
// For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
};
use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedUint64},
};

// const (
// 	gasConstraintTargetOffset uint64 = iota
// 	gasConstraintAdjustmentWindowOffset
// 	gasConstraintBacklogOffset
// )
const TARGET_OFFSET: u64 = 0;
const ADJUSTMENT_WINDOW_OFFSET: u64 = 1;
const BACKLOG_OFFSET: u64 = 2;

// type GasConstraint struct {
// 	target           storage.StorageBackedUint64
// 	adjustmentWindow storage.StorageBackedUint64
// 	backlog          storage.StorageBackedUint64
// }
pub struct GasConstraint<B: Burner> {
    target: StorageBackedUint64<B>,
    adjustment_window: StorageBackedUint64<B>,
    backlog: StorageBackedUint64<B>,
}

impl<B: Burner> GasConstraint<B> {
    // func OpenGasConstraint(storage *storage.Storage) *GasConstraint
    pub fn open(sto: &Storage<B>) -> Self
    where
        B: Clone,
    {
        GasConstraint {
            target: sto.open_storage_backed_uint64(TARGET_OFFSET),
            adjustment_window: sto.open_storage_backed_uint64(ADJUSTMENT_WINDOW_OFFSET),
            backlog: sto.open_storage_backed_uint64(BACKLOG_OFFSET),
        }
    }

    // func (c *GasConstraint) Clear() error
    pub fn clear<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.target.clear(ctx)?;
        self.adjustment_window.clear(ctx)?;
        self.backlog.clear(ctx)
    }

    // func (c *GasConstraint) Target() (uint64, error)
    pub fn target<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.target.get(db)
    }

    // func (c *GasConstraint) SetTarget(val uint64) error
    pub fn set_target<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.target.set(ctx, val)
    }

    // func (c *GasConstraint) AdjustmentWindow() (uint64, error)
    pub fn adjustment_window<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.adjustment_window.get(db)
    }

    // func (c *GasConstraint) SetAdjustmentWindow(val uint64) error
    pub fn set_adjustment_window<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.adjustment_window.set(ctx, val)
    }

    // func (c *GasConstraint) Backlog() (uint64, error)
    pub fn backlog<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.backlog.get(db)
    }

    // func (c *GasConstraint) SetBacklog(val uint64) error
    pub fn set_backlog<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.backlog.set(ctx, val)
    }
}
