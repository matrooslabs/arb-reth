// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package storage

// import (
// 	"github.com/ethereum/go-ethereum/common"

// 	"github.com/offchainlabs/nitro/arbos/util"
// )

// type Queue struct {
// 	storage       *Storage
// 	nextPutOffset StorageBackedUint64
// 	nextGetOffset StorageBackedUint64
// }

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{StorageKey, StorageValue},
};
use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedUint64},
};

pub struct Queue<B: Burner> {
    storage: Storage<B>,
    next_put_offset: StorageBackedUint64<B>,
    next_get_offset: StorageBackedUint64<B>,
}

impl<B: Burner> Queue<B> {
    // func InitializeQueue(sto *Storage) error
    pub fn initialize<CTX: ContextTr>(
        sto: &Storage<B>,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        sto.set_uint64_by_uint64(ctx, 0, 2)?;
        sto.set_uint64_by_uint64(ctx, 1, 2)
    }

    // func OpenQueue(sto *Storage) *Queue
    pub fn open(sto: &Storage<B>) -> Self
    where
        B: Clone,
    {
        Queue {
            storage: Storage {
                account: sto.account,
                storage_key: sto.storage_key.clone(),
                burner: sto.burner.clone(),
                hash_cache: None,
            },
            next_put_offset: sto.open_storage_backed_uint64(0),
            next_get_offset: sto.open_storage_backed_uint64(1),
        }
    }

    // func (q *Queue) IsEmpty() (bool, error)
    pub fn is_empty<Db: Database>(&self, db: &mut Db) -> Result<bool, Db::Error> {
        let put = self.next_put_offset.get(db)?;
        let get = self.next_get_offset.get(db)?;
        Ok(put == get)
    }

    // func (q *Queue) Size() (uint64, error)
    pub fn size<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        let put = self.next_put_offset.get(db)?;
        let get = self.next_get_offset.get(db)?;
        Ok(put - get)
    }

    // func (q *Queue) Peek() (*common.Hash, error) -- returns None iff empty
    pub fn peek<Db: Database>(&self, db: &mut Db) -> Result<Option<StorageValue>, Db::Error> {
        if self.is_empty(db)? {
            return Ok(None);
        }
        let next = self.next_get_offset.get(db)?;
        let res = self.storage.get_by_uint64(db, next)?;
        Ok(Some(res))
    }

    // func (q *Queue) Get() (*common.Hash, error) -- returns None iff empty
    pub fn get<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<Option<StorageValue>, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        if self.is_empty(ctx.db_mut())? {
            return Ok(None);
        }
        let new_offset = self.next_get_offset.increment(ctx)?;
        // Swap with zero to clear the slot (like Go's Swap(UintToHash(newOffset-1), common.Hash{}))
        let res = self.storage.swap(ctx, StorageKey::from(new_offset - 1), StorageValue::ZERO)?;
        Ok(Some(res))
    }

    // func (q *Queue) Put(val common.Hash) error
    pub fn put<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: StorageValue,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let new_offset = self.next_put_offset.increment(ctx)?;
        self.storage.set_by_uint64(ctx, new_offset - 1, val)
    }

    // func (q *Queue) Shift() (bool, error)
    // Resets the queue to its starting state if empty. Returns true iff reset was done.
    pub fn shift<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<bool, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let put = self.next_put_offset.get(ctx.db_mut())?;
        let get = self.next_get_offset.get(ctx.db_mut())?;
        if put != get {
            return Ok(false);
        }
        self.next_get_offset.set(ctx, 2)?;
        self.next_put_offset.set(ctx, 2)?;
        Ok(true)
    }

    // func (q *Queue) ForEach(closure func(uint64, common.Hash) (bool, error)) error
    pub fn for_each<Db: Database, F>(&self, db: &mut Db, mut closure: F) -> Result<(), Db::Error>
    where
        F: FnMut(u64, StorageValue) -> Result<bool, Db::Error>,
    {
        let size = self.size(db)?;
        let offset = self.next_get_offset.get(db)?;
        for index in 0..size {
            let entry = self.storage.get_by_uint64(db, offset + index)?;
            if closure(index, entry)? {
                return Ok(());
            }
        }
        Ok(())
    }
}
