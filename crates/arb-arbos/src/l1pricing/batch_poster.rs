// // Copyright 2022-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package l1pricing

// import (
// 	"errors"
// 	"math"
// 	"math/big"

// 	"github.com/ethereum/go-ethereum/common"

// 	"github.com/offchainlabs/nitro/arbos/addressSet"
// 	"github.com/offchainlabs/nitro/arbos/storage"
// 	"github.com/offchainlabs/nitro/util/arbmath"
// )

// const totalFundsDueOffset = 0

// var (
// 	PosterAddrsKey = []byte{0}
// 	PosterInfoKey  = []byte{1}

// 	ErrAlreadyExists = errors.New("tried to add a batch poster that already exists")
// 	ErrNotExist      = errors.New("tried to open a batch poster that does not exist")
// )

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{Address, I256},
};

use crate::{
    address_set::AddressSet,
    burn::Burner,
    storage::storage::{Storage, StorageBackedAddress, StorageBackedBigInt},
};

// const totalFundsDueOffset = 0
// var PosterAddrsKey = []byte{0}
// var PosterInfoKey  = []byte{1}
const TOTAL_FUNDS_DUE_OFFSET: u64 = 0;
const POSTER_ADDRS_KEY: &[u8] = &[0];
const POSTER_INFO_KEY: &[u8] = &[1];

/// Errors returned by `BatchPostersTable` operations that can fail for semantic
/// (not just database I/O) reasons.
///
/// Maps to Go's package-level `ErrAlreadyExists` and `ErrNotExist`.
#[derive(Debug, PartialEq, Eq)]
pub enum BatchPosterError {
    /// Returned by `add_poster` when the address is already in the set.
    AlreadyExists,
    /// Returned by `open_poster` when `create_if_not_exist` is false and
    /// the poster does not exist.
    NotExist,
}

// // BatchPostersTable is the layout of storage in the table
// type BatchPostersTable struct {
// 	posterAddrs   *addressSet.AddressSet
// 	posterInfo    *storage.Storage
// 	totalFundsDue storage.StorageBackedBigInt
// }
pub struct BatchPostersTable<B: Burner> {
    poster_addrs: AddressSet<B>,
    poster_info: Storage<B>,
    total_funds_due: StorageBackedBigInt<B>,
}

// type BatchPosterState struct {
// 	fundsDue     storage.StorageBackedBigInt
// 	payTo        storage.StorageBackedAddress
// 	postersTable *BatchPostersTable
// }
pub struct BatchPosterState<B: Burner> {
    funds_due: StorageBackedBigInt<B>,
    pay_to: StorageBackedAddress<B>,
    // Replaces Go's `postersTable *BatchPostersTable` back-pointer.
    // A clone of the table-level `total_funds_due` slot. Both this and the
    // original view in `BatchPostersTable` address the same EVM slot, so
    // mutations through either are visible to the other (they share state
    // via the EVM journal).
    table_total_funds_due: StorageBackedBigInt<B>,
}

// type FundsDueItem struct {
// 	dueTo   common.Address
// 	balance *big.Int
// }
pub struct FundsDueItem {
    pub due_to: Address,
    pub balance: I256,
}

impl<B: Burner> BatchPostersTable<B> {
    /// Zeroes out `totalFundsDue` and initialises the poster address set.
    /// Call once on a fresh storage before opening the table via `open`.
    ///
    /// Maps to Go's package-level `InitializeBatchPostersTable`.
    // func InitializeBatchPostersTable(storage *storage.Storage) error {
    // 	totalFundsDue := storage.OpenStorageBackedBigInt(totalFundsDueOffset)
    // 	if err := totalFundsDue.SetChecked(common.Big0); err != nil {
    // 		return err
    // 	}
    // 	return addressSet.Initialize(storage.OpenCachedSubStorage(PosterAddrsKey))
    // }
    pub fn initialize<CTX: ContextTr>(
        storage: &Storage<B>,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error>
    where
        B: Clone,
    {
        let mut total_funds_due = storage.open_storage_backed_big_int(TOTAL_FUNDS_DUE_OFFSET);
        total_funds_due.set_checked(ctx, I256::ZERO)?;
        let mut poster_addrs_storage = storage.open_cached_sub_storage(POSTER_ADDRS_KEY);
        AddressSet::initialize(&mut poster_addrs_storage, ctx)
    }

    /// Opens an existing `BatchPostersTable` from `storage`.
    ///
    /// Maps to Go's package-level `OpenBatchPostersTable`.
    // func OpenBatchPostersTable(storage *storage.Storage) *BatchPostersTable {
    //     return &BatchPostersTable{
    //         posterAddrs:   addressSet.OpenAddressSet(storage.OpenCachedSubStorage(PosterAddrsKey)),
    //         posterInfo:    storage.OpenSubStorage(PosterInfoKey),
    //         totalFundsDue: storage.OpenStorageBackedBigInt(totalFundsDueOffset),
    //     }
    // }
    pub fn open(storage: &Storage<B>) -> Self
    where
        B: Clone,
    {
        BatchPostersTable {
            poster_addrs: AddressSet::new(storage.open_cached_sub_storage(POSTER_ADDRS_KEY)),
            poster_info: storage.open_sub_storage(POSTER_INFO_KEY),
            total_funds_due: storage.open_storage_backed_big_int(TOTAL_FUNDS_DUE_OFFSET),
        }
    }

    /// Opens a `BatchPosterState` for `poster` without checking membership.
    ///
    /// Maps to Go's `(*BatchPostersTable).internalOpen`.
    // func (bpt *BatchPostersTable) internalOpen(poster common.Address) *BatchPosterState {
    // 	bpStorage := bpt.posterInfo.OpenSubStorage(poster.Bytes())
    // 	return &BatchPosterState{
    // 		fundsDue:     bpStorage.OpenStorageBackedBigInt(0),
    // 		payTo:        bpStorage.OpenStorageBackedAddress(1),
    // 		postersTable: bpt,
    // 	}
    // }
    fn internal_open(&self, poster: Address) -> BatchPosterState<B>
    where
        B: Clone,
    {
        let bp_storage = self.poster_info.open_sub_storage(poster.as_slice());
        BatchPosterState {
            funds_due: bp_storage.open_storage_backed_big_int(0),
            pay_to: bp_storage.open_storage_backed_address(1),
            // Clone the table-level slot so set_funds_due can update the
            // aggregate without needing a back-pointer to BatchPostersTable.
            table_total_funds_due: self.total_funds_due.clone(),
        }
    }

    /// Opens (and optionally creates) a `BatchPosterState` for `poster`.
    ///
    /// Returns `Err(BatchPosterError::NotExist)` if the poster does not exist
    /// and `create_if_not_exist` is `false`.
    ///
    /// Maps to Go's `(*BatchPostersTable).OpenPoster`.
    // func (bpt *BatchPostersTable) OpenPoster(poster common.Address, createIfNotExist bool) (*BatchPosterState, error) {
    // 	isBatchPoster, err := bpt.posterAddrs.IsMember(poster)
    // 	if err != nil {
    // 		return nil, err
    // 	}
    // 	if !isBatchPoster {
    // 		if !createIfNotExist {
    // 			return nil, ErrNotExist
    // 		}
    // 		return bpt.AddPoster(poster, poster)
    // 	}
    // 	return bpt.internalOpen(poster), nil
    // }
    pub fn open_poster<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        poster: Address,
        create_if_not_exist: bool,
    ) -> Result<
        BatchPosterState<B>,
        OpenPosterResult<<<CTX::Journal as JournalTr>::Database as Database>::Error>,
    >
    where
        B: Clone,
    {
        let is_batch_poster = self
            .poster_addrs
            .is_member(ctx.db_mut(), poster)
            .map_err(OpenPosterResult::Db)?;
        if !is_batch_poster {
            if !create_if_not_exist {
                return Err(OpenPosterResult::Semantic(BatchPosterError::NotExist));
            }
            return self.add_poster(ctx, poster, poster);
        }
        Ok(self.internal_open(poster))
    }

    /// Returns `true` if `poster` is in the batch poster set.
    ///
    /// Maps to Go's `(*BatchPostersTable).ContainsPoster`.
    // func (bpt *BatchPostersTable) ContainsPoster(poster common.Address) (bool, error) {
    // 	return bpt.posterAddrs.IsMember(poster)
    // }
    pub fn contains_poster<Db: Database>(
        &self,
        db: &mut Db,
        poster: Address,
    ) -> Result<bool, Db::Error> {
        self.poster_addrs.is_member(db, poster)
    }

    /// Adds `poster_address` to the set with the given `pay_to` recipient, then
    /// returns its initialised state.
    ///
    /// Returns `Err(BatchPosterError::AlreadyExists)` if the address is already a member.
    ///
    /// Maps to Go's `(*BatchPostersTable).AddPoster`.
    // func (bpt *BatchPostersTable) AddPoster(posterAddress common.Address, payTo common.Address) (*BatchPosterState, error) {
    // 	isBatchPoster, err := bpt.posterAddrs.IsMember(posterAddress)
    // 	if err != nil {
    // 		return nil, err
    // 	}
    // 	if isBatchPoster {
    // 		return nil, ErrAlreadyExists
    // 	}
    // 	bpState := bpt.internalOpen(posterAddress)
    // 	if err := bpState.fundsDue.SetChecked(common.Big0); err != nil {
    // 		return nil, err
    // 	}
    // 	if err := bpState.payTo.Set(payTo); err != nil {
    // 		return nil, err
    // 	}
    // 	if err := bpt.posterAddrs.Add(posterAddress); err != nil {
    // 		return nil, err
    // 	}
    // 	return bpState, nil
    // }
    pub fn add_poster<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        poster_address: Address,
        pay_to: Address,
    ) -> Result<
        BatchPosterState<B>,
        OpenPosterResult<<<CTX::Journal as JournalTr>::Database as Database>::Error>,
    >
    where
        B: Clone,
    {
        let is_batch_poster = self
            .poster_addrs
            .is_member(ctx.db_mut(), poster_address)
            .map_err(OpenPosterResult::Db)?;
        if is_batch_poster {
            return Err(OpenPosterResult::Semantic(BatchPosterError::AlreadyExists));
        }
        let mut bp_state = self.internal_open(poster_address);
        bp_state
            .funds_due
            .set_checked(ctx, I256::ZERO)
            .map_err(OpenPosterResult::Db)?;
        bp_state.pay_to.set(ctx, pay_to).map_err(OpenPosterResult::Db)?;
        self.poster_addrs.add(ctx, poster_address).map_err(OpenPosterResult::Db)?;
        Ok(bp_state)
    }

    /// Returns up to `max_num_to_get` poster addresses in insertion order.
    ///
    /// Maps to Go's `(*BatchPostersTable).AllPosters`.
    // func (bpt *BatchPostersTable) AllPosters(maxNumToGet uint64) ([]common.Address, error) {
    // 	return bpt.posterAddrs.AllMembers(maxNumToGet)
    // }
    pub fn all_posters<Db: Database>(
        &mut self,
        db: &mut Db,
        max_num_to_get: u64,
    ) -> Result<Vec<Address>, Db::Error> {
        self.poster_addrs.all_members(db, max_num_to_get)
    }

    /// Returns the aggregate funds due across all batch posters.
    ///
    /// Maps to Go's `(*BatchPostersTable).TotalFundsDue`.
    // func (bpt *BatchPostersTable) TotalFundsDue() (*big.Int, error) {
    // 	return bpt.totalFundsDue.Get()
    // }
    pub fn total_funds_due<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        self.total_funds_due.get(db)
    }

    /// Returns all posters with a positive balance owed.
    ///
    /// Maps to Go's `(*BatchPostersTable).GetFundsDueList`.
    // func (bpt *BatchPostersTable) GetFundsDueList() ([]FundsDueItem, error) {
    // 	ret := []FundsDueItem{}
    // 	allPosters, err := bpt.AllPosters(math.MaxUint64)
    // 	if err != nil {
    // 		return nil, err
    // 	}
    // 	for _, posterAddr := range allPosters {
    // 		poster, err := bpt.OpenPoster(posterAddr, false)
    // 		if err != nil {
    // 			return nil, err
    // 		}
    // 		due, err := poster.FundsDue()
    // 		if err != nil {
    // 			return nil, err
    // 		}
    // 		if due.Sign() > 0 {
    // 			ret = append(ret, FundsDueItem{
    // 				dueTo:   posterAddr,
    // 				balance: due,
    // 			})
    // 		}
    // 	}
    // 	return ret, nil
    // }
    pub fn get_funds_due_list<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<
        Vec<FundsDueItem>,
        OpenPosterResult<<<CTX::Journal as JournalTr>::Database as Database>::Error>,
    >
    where
        B: Clone,
    {
        // Collect all addresses first (owned Vec), so there's no live borrow
        // on `self` while we iterate and call open_poster.
        let all_posters = self
            .all_posters(ctx.db_mut(), u64::MAX)
            .map_err(OpenPosterResult::Db)?;

        let mut ret = Vec::new();
        for poster_addr in all_posters {
            let poster = self.open_poster(ctx, poster_addr, false)?;
            let due = poster.funds_due(ctx.db_mut()).map_err(OpenPosterResult::Db)?;
            if due.is_positive() {
                ret.push(FundsDueItem { due_to: poster_addr, balance: due });
            }
        }
        Ok(ret)
    }
}

impl<B: Burner> BatchPosterState<B> {
    /// Returns the funds owed to this poster.
    ///
    /// Maps to Go's `(*BatchPosterState).FundsDue`.
    // func (bps *BatchPosterState) FundsDue() (*big.Int, error) {
    // 	return bps.fundsDue.Get()
    // }
    pub fn funds_due<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        self.funds_due.get(db)
    }

    /// Updates this poster's funds due and adjusts the table-level aggregate.
    ///
    /// The aggregate is updated as: `new_total = prev_total + val - prev`, which
    /// matches Go's `arbmath.BigSub(arbmath.BigAdd(prevTotal, val), prev)`.
    ///
    /// Maps to Go's `(*BatchPosterState).SetFundsDue`.
    // func (bps *BatchPosterState) SetFundsDue(val *big.Int) error {
    // 	fundsDue := bps.fundsDue
    // 	totalFundsDue := bps.postersTable.totalFundsDue
    // 	prev, err := fundsDue.Get()
    // 	if err != nil {
    // 		return err
    // 	}
    // 	prevTotal, err := totalFundsDue.Get()
    // 	if err != nil {
    // 		return err
    // 	}
    // 	if err := totalFundsDue.SetSaturatingWithWarning(arbmath.BigSub(arbmath.BigAdd(prevTotal, val), prev), "batch poster total funds due"); err != nil {
    // 		return err
    // 	}
    // 	return bps.fundsDue.SetSaturatingWithWarning(val, "batch poster funds due")
    // }
    pub fn set_funds_due<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: I256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let prev = self.funds_due.get(ctx.db_mut())?;
        let prev_total = self.table_total_funds_due.get(ctx.db_mut())?;
        // new_total = prevTotal + val - prev  (matches Go's arbmath.BigSub(BigAdd(...)))
        let new_total = prev_total.saturating_add(val).saturating_sub(prev);
        self.table_total_funds_due
            .set_saturating_with_warning(ctx, new_total, "batch poster total funds due")?;
        self.funds_due.set_saturating_with_warning(ctx, val, "batch poster funds due")
    }

    /// Returns the address that receives payment for this poster.
    ///
    /// Maps to Go's `(*BatchPosterState).PayTo`.
    // func (bps *BatchPosterState) PayTo() (common.Address, error) {
    // 	return bps.payTo.Get()
    // }
    pub fn pay_to<Db: Database>(&self, db: &mut Db) -> Result<Address, Db::Error> {
        self.pay_to.get(db)
    }

    /// Sets the payment-recipient address for this poster.
    ///
    /// Maps to Go's `(*BatchPosterState).SetPayTo`.
    // func (bps *BatchPosterState) SetPayTo(addr common.Address) error {
    // 	return bps.payTo.Set(addr)
    // }
    pub fn set_pay_to<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        addr: Address,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.pay_to.set(ctx, addr)
    }
}

/// Combined error type for `BatchPostersTable` operations that can fail with
/// either a database I/O error or a semantic batch-poster error.
///
/// Use `.map_err(OpenPosterResult::Db)?` to propagate database errors into
/// this type at call sites.
#[derive(Debug)]
pub enum OpenPosterResult<E> {
    Semantic(BatchPosterError),
    Db(E),
}
