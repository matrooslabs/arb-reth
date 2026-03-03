// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package storage

// import (
// 	"bytes"
// 	"fmt"
// 	"math"
// 	"math/big"
// 	"sync/atomic"

// 	"github.com/ethereum/go-ethereum/arbitrum/multigas"
// 	"github.com/ethereum/go-ethereum/common"
// 	"github.com/ethereum/go-ethereum/common/lru"
// 	"github.com/ethereum/go-ethereum/core/rawdb"
// 	"github.com/ethereum/go-ethereum/core/state"
// 	"github.com/ethereum/go-ethereum/core/tracing"
// 	"github.com/ethereum/go-ethereum/core/types"
// 	"github.com/ethereum/go-ethereum/core/vm"
// 	"github.com/ethereum/go-ethereum/crypto"
// 	"github.com/ethereum/go-ethereum/log"
// 	"github.com/ethereum/go-ethereum/params"
// 	"github.com/ethereum/go-ethereum/triedb"
// 	"github.com/ethereum/go-ethereum/triedb/hashdb"
// 	"github.com/ethereum/go-ethereum/triedb/pathdb"

// 	"github.com/offchainlabs/nitro/arbos/burn"
// 	"github.com/offchainlabs/nitro/arbos/util"
// 	"github.com/offchainlabs/nitro/util/arbmath"
// 	"github.com/offchainlabs/nitro/util/testhelpers/env"
// )

// // Storage allows ArbOS to store data persistently in the Ethereum-compatible stateDB. This is represented in
// // the stateDB as the storage of a fictional Ethereum account at address 0xA4B05FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF.
// //
// // The storage is logically a tree of storage spaces which can be nested hierarchically, with each storage space
// // containing a key-value store with 256-bit keys and values. Uninitialized storage spaces and uninitialized keys
// // within initialized storage spaces are deemed to be filled with zeroes (consistent with the behavior of Ethereum
// // account storage). Logically, when a chain is launched, all possible storage spaces and all possible keys within
// // them exist and contain zeroes.
// //
// // A storage space (represented by a Storage object) has a byte-slice storageKey which distinguishes it from other
// // storage spaces. The root Storage has its storageKey as the empty string. A parent storage space can contain children,
// // each with a distinct name. The storageKey of a child is keccak256(parent.storageKey, name). Note that two spaces
// // cannot have the same storageKey because that would imply a collision in keccak256.
// //
// // The contents of all storage spaces are stored in a single, flat key-value store that is implemented as the storage
// // of the fictional Ethereum account. The contents of key, within a storage space with storageKey, are stored
// // at location keccak256(storageKey, key) in the flat KVS. Two slots, whether in the same or different storage spaces,
// // cannot occupy the same location because that would imply a collision in keccak256.

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{Address, I256, StorageKey, StorageValue, U256, keccak256},
};

use crate::arbos::burn::Burner;

// type Storage struct {
// 	account    common.Address
// 	db         vm.StateDB
// 	storageKey []byte
// 	burner     burn.Burner
// 	hashCache  *lru.Cache[string, []byte]
// }

// const StorageReadCost = params.SloadGasEIP2200
// const StorageWriteCost = params.SstoreSetGasEIP2200
// const StorageWriteZeroCost = params.SstoreResetGasEIP2200
// const StorageCodeHashCost = params.ColdAccountAccessCostEIP2929

// const storageKeyCacheSize = 1024

// var storageHashCache = lru.NewCache[string, []byte](storageKeyCacheSize)
// var cacheFullLogged atomic.Bool

// // KVStorage uses a Geth database to create an evm key-value store for an arbitrary account.
// func KVStorage(statedb vm.StateDB, burner burn.Burner, account common.Address) *Storage {
// 	statedb.SetNonce(account, 1, tracing.NonceChangeUnspecified) // ensures Geth won't treat the account as empty
// 	return &Storage{
// 		account:    account,
// 		db:         statedb,
// 		storageKey: []byte{},
// 		burner:     burner,
// 		hashCache:  storageHashCache,
// 	}
// }

// // NewGeth uses a Geth database to create an evm key-value store backed by the ArbOS state account.
// func NewGeth(statedb vm.StateDB, burner burn.Burner) *Storage {
// 	return KVStorage(statedb, burner, types.ArbosStateAddress)
// }

// // FilteredTransactionsStorage creates an evm key-value store backed by the dedicated filtered tx state account.
// func FilteredTransactionsStorage(statedb vm.StateDB, burner burn.Burner) *Storage {
// 	return KVStorage(statedb, burner, types.FilteredTransactionsStateAddress)
// }

// // NewMemoryBacked uses Geth's memory-backed database to create an evm key-value store.
// // Only used for testing.
// func NewMemoryBacked(burner burn.Burner) *Storage {
// 	return NewGeth(NewMemoryBackedStateDB(), burner)
// }

// // NewMemoryBackedStateDB uses Geth's memory-backed database to create a statedb
// // Only used for testing.
// func NewMemoryBackedStateDB() vm.StateDB {
// 	raw := rawdb.NewMemoryDatabase()
// 	trieConfig := &triedb.Config{Preimages: false, PathDB: pathdb.Defaults}
// 	if env.GetTestStateScheme() == rawdb.HashScheme {
// 		trieConfig = &triedb.Config{Preimages: false, HashDB: hashdb.Defaults}
// 	}
// 	db := state.NewDatabase(triedb.NewDatabase(raw, trieConfig), nil)
// 	statedb, err := state.New(common.Hash{}, db)
// 	if err != nil {
// 		panic("failed to init empty statedb")
// 	}
// 	return statedb
// }

// // We map addresses using "pages" of 256 storage slots. We hash over the page number but not the offset within
// // a page, to preserve contiguity within a page. This will reduce cost if/when Ethereum switches to storage
// // representations that reward contiguity.
// // Because page numbers are 248 bits, this gives us 124-bit security against collision attacks, which is good enough.
// func (s *Storage) mapAddress(key common.Hash) common.Hash {
// 	keyBytes := key.Bytes()
// 	boundary := common.HashLength - 1
// 	mapped := make([]byte, 0, common.HashLength)
// 	mapped = append(mapped, s.cachedKeccak(s.storageKey, keyBytes[:boundary])[:boundary]...)
// 	mapped = append(mapped, keyBytes[boundary])
// 	return common.BytesToHash(mapped)
// }

// func writeCost(value common.Hash) uint64 {
// 	if value == (common.Hash{}) {
// 		return StorageWriteZeroCost
// 	}
// 	return StorageWriteCost
// }

// func (s *Storage) Account() common.Address {
// 	return s.account
// }

// func (s *Storage) Get(key common.Hash) (common.Hash, error) {
// 	err := s.burner.Burn(multigas.ResourceKindStorageAccess, StorageReadCost)
// 	if err != nil {
// 		return common.Hash{}, err
// 	}
// 	if info := s.burner.TracingInfo(); info != nil {
// 		info.RecordStorageGet(s.mapAddress(key))
// 	}
// 	return s.GetFree(key), nil
// }

// // Gets a storage slot for free. Dangerous due to DoS potential.
// func (s *Storage) GetFree(key common.Hash) common.Hash {
// 	return s.db.GetState(s.account, s.mapAddress(key))
// }

// // ClearFree deletes a storage slot without charging gas. Setting a slot to
// // common.Hash{} (all zeros) causes geth to delete the entry from the storage
// // trie rather than storing zeros (see state_object.go updateTrie).
// // Dangerous due to DoS potential - only use for consensus-critical cleanup.
// func (s *Storage) ClearFree(key common.Hash) {
// 	if info := s.burner.TracingInfo(); info != nil {
// 		info.RecordStorageSet(s.mapAddress(key), common.Hash{})
// 	}
// 	s.db.SetState(s.account, s.mapAddress(key), common.Hash{})
// }

// func (s *Storage) GetStorageSlot(key common.Hash) common.Hash {
// 	return s.mapAddress(key)
// }

// func (s *Storage) GetUint64(key common.Hash) (uint64, error) {
// 	value, err := s.Get(key)
// 	return value.Big().Uint64(), err
// }

// func (s *Storage) GetByUint64(key uint64) (common.Hash, error) {
// 	return s.Get(util.UintToHash(key))
// }

// func (s *Storage) GetUint64ByUint64(key uint64) (uint64, error) {
// 	return s.GetUint64(util.UintToHash(key))
// }

// func (s *Storage) Set(key common.Hash, value common.Hash) error {
// 	if s.burner.ReadOnly() {
// 		log.Error("Read-only burner attempted to mutate state", "key", key, "value", value)
// 		return vm.ErrWriteProtection
// 	}
// 	err := s.burner.Burn(multigas.ResourceKindStorageAccess, writeCost(value))
// 	if err != nil {
// 		return err
// 	}
// 	if info := s.burner.TracingInfo(); info != nil {
// 		info.RecordStorageSet(s.mapAddress(key), value)
// 	}
// 	s.db.SetState(s.account, s.mapAddress(key), value)
// 	return nil
// }

// func (s *Storage) SetUint64(key common.Hash, value uint64) error {
// 	return s.Set(key, util.UintToHash(value))
// }

// func (s *Storage) SetByUint64(key uint64, value common.Hash) error {
// 	return s.Set(util.UintToHash(key), value)
// }

// func (s *Storage) SetUint64ByUint64(key uint64, value uint64) error {
// 	return s.Set(util.UintToHash(key), util.UintToHash(value))
// }

// func (s *Storage) SetUint32(key common.Hash, value uint32) error {
// 	return s.Set(key, util.UintToHash(uint64(value)))
// }

// func (s *Storage) SetByUint32(key uint32, value common.Hash) error {
// 	return s.Set(util.UintToHash(uint64(key)), value)
// }

// func (s *Storage) Clear(key common.Hash) error {
// 	return s.Set(key, common.Hash{})
// }

// func (s *Storage) ClearByUint64(key uint64) error {
// 	return s.Set(util.UintToHash(key), common.Hash{})
// }

// func (s *Storage) Swap(key common.Hash, newValue common.Hash) (common.Hash, error) {
// 	oldValue, err := s.Get(key)
// 	if err != nil {
// 		return common.Hash{}, err
// 	}
// 	return oldValue, s.Set(key, newValue)
// }

// func (s *Storage) OpenCachedSubStorage(id []byte) *Storage {
// 	return &Storage{
// 		account:    s.account,
// 		db:         s.db,
// 		storageKey: s.cachedKeccak(s.storageKey, id),
// 		burner:     s.burner,
// 		hashCache:  storageHashCache,
// 	}
// }
// func (s *Storage) OpenSubStorage(id []byte) *Storage {
// 	return &Storage{
// 		account:    s.account,
// 		db:         s.db,
// 		storageKey: s.cachedKeccak(s.storageKey, id),
// 		burner:     s.burner,
// 		hashCache:  nil,
// 	}
// }

// // Returns shallow copy of Storage that won't use storage key hash cache.
// // The storage space represented by the returned Storage is kept the same.
// func (s *Storage) WithoutCache() *Storage {
// 	return &Storage{
// 		account:    s.account,
// 		db:         s.db,
// 		storageKey: s.storageKey,
// 		burner:     s.burner,
// 		hashCache:  nil,
// 	}
// }

// func (s *Storage) SetBytes(b []byte) error {
// 	err := s.ClearBytes()
// 	if err != nil {
// 		return err
// 	}
// 	err = s.SetUint64ByUint64(0, uint64(len(b)))
// 	if err != nil {
// 		return err
// 	}
// 	offset := uint64(1)
// 	for len(b) >= 32 {
// 		err = s.SetByUint64(offset, common.BytesToHash(b[:32]))
// 		if err != nil {
// 			return err
// 		}
// 		b = b[32:]
// 		offset++
// 	}
// 	return s.SetByUint64(offset, common.BytesToHash(b))
// }

// func (s *Storage) GetBytes() ([]byte, error) {
// 	bytesLeft, err := s.GetUint64ByUint64(0)
// 	if err != nil {
// 		return nil, err
// 	}
// 	ret := []byte{}
// 	offset := uint64(1)
// 	for bytesLeft >= 32 {
// 		next, err := s.GetByUint64(offset)
// 		if err != nil {
// 			return nil, err
// 		}
// 		ret = append(ret, next.Bytes()...)
// 		bytesLeft -= 32
// 		offset++
// 	}
// 	next, err := s.GetByUint64(offset)
// 	if err != nil {
// 		return nil, err
// 	}
// 	ret = append(ret, next.Bytes()[32-bytesLeft:]...)
// 	return ret, nil
// }

// func (s *Storage) GetBytesSize() (uint64, error) {
// 	return s.GetUint64ByUint64(0)
// }

// func (s *Storage) ClearBytes() error {
// 	bytesLeft, err := s.GetUint64ByUint64(0)
// 	if err != nil {
// 		return err
// 	}
// 	offset := uint64(1)
// 	for bytesLeft > 0 {
// 		err := s.ClearByUint64(offset)
// 		if err != nil {
// 			return err
// 		}
// 		offset++
// 		if bytesLeft < 32 {
// 			bytesLeft = 0
// 		} else {
// 			bytesLeft -= 32
// 		}
// 	}
// 	return s.ClearByUint64(0)
// }

// func (s *Storage) GetCodeHash(address common.Address) (common.Hash, error) {
// 	err := s.burner.Burn(multigas.ResourceKindStorageAccess, StorageCodeHashCost)
// 	if err != nil {
// 		return common.Hash{}, err
// 	}
// 	return s.db.GetCodeHash(address), nil
// }

// func (s *Storage) Burner() burn.Burner {
// 	return s.burner // not public because these should never be changed once set
// }

// func (s *Storage) Keccak(data ...[]byte) ([]byte, error) {
// 	var byteCount uint64
// 	for _, part := range data {
// 		byteCount += uint64(len(part))
// 	}
// 	cost := 30 + 6*arbmath.WordsForBytes(byteCount)
// 	if err := s.burner.Burn(multigas.ResourceKindComputation, cost); err != nil {
// 		return nil, err
// 	}
// 	return crypto.Keccak256(data...), nil
// }

// func (s *Storage) KeccakHash(data ...[]byte) (common.Hash, error) {
// 	bytes, err := s.Keccak(data...)
// 	return common.BytesToHash(bytes), err
// }

// // Returns crypto.Keccak256 result for the given data
// // If available the result is taken from hash cache
// // otherwise crypto.Keccak256 is executed and its result is added to the cache and returned
// // note: the method doesn't burn gas, as it's only intended for generating storage subspace keys and mapping slot addresses
// // note: returned slice is not thread-safe
// func (s *Storage) cachedKeccak(data ...[]byte) []byte {
// 	if s.hashCache == nil {
// 		return crypto.Keccak256(data...)
// 	}
// 	keyString := string(bytes.Join(data, []byte{}))
// 	if hash, wasCached := s.hashCache.Get(keyString); wasCached {
// 		return hash
// 	}
// 	hash := crypto.Keccak256(data...)
// 	evicted := s.hashCache.Add(keyString, hash)
// 	if evicted && cacheFullLogged.CompareAndSwap(false, true) {
// 		log.Warn("Hash cache full, we didn't expect that. Some non-static storage keys may fill up the cache.")
// 	}
// 	return hash
// }

// type StorageSlot struct {
// 	account common.Address
// 	db      vm.StateDB
// 	slot    common.Hash
// 	burner  burn.Burner
// }

/// A single EVM storage slot belonging to `account` at mapped key `slot`.
///
/// The database handle is **not** stored here; instead it is passed at each
/// call site (Option B). This mirrors how revm itself threads context through
/// calls rather than storing it in sub-objects.
pub struct StorageSlot<B: Burner> {
    account: Address,
    slot: StorageKey,
    burner: B,
}

impl<B: Burner> StorageSlot<B> {
    pub fn new(account: Address, slot: StorageKey, burner: B) -> Self {
        Self {
            account,
            slot,
            burner,
        }
    }

    /// Reads the raw slot value from the backing database.
    ///
    /// Maps to Go's `StorageSlot.Get` — uses the read-only `Database` trait
    /// so this can be called without a full EVM context.
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<StorageValue, Db::Error> {
        // TODO: burner logic — charge StorageReadCost via self.burner.burn(ResourceKindStorageAccess, STORAGE_READ_COST)
        //       and record self.burner.tracing_info().record_storage_get(self.slot) if tracing is enabled.
        db.storage(self.account, self.slot)
    }

    /// Writes a value through the EVM journal so the change is tracked for
    /// reverts, warming, and dirty-slot accounting.
    ///
    /// Maps to Go's `StorageSlot.Set` — requires a mutable EVM context
    /// because all writes must go through `Journal::sstore`.
    pub fn set<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        value: StorageValue,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        // TODO: burner logic — guard with self.burner.read_only(), charge write_cost(value) via
        //       self.burner.burn(ResourceKindStorageAccess, cost), and record
        //       self.burner.tracing_info().record_storage_set(self.slot, value) if tracing is enabled.
        ctx.journal_mut()
            .sstore(self.account, self.slot, value)
            .map(|_| ())
    }
}

// func (s *Storage) NewSlot(offset uint64) StorageSlot {
// 	return StorageSlot{s.account, s.db, s.mapAddress(util.UintToHash(offset)), s.burner}
// }

// func (ss *StorageSlot) Get() (common.Hash, error) {
// 	err := ss.burner.Burn(multigas.ResourceKindStorageAccess, StorageReadCost)
// 	if err != nil {
// 		return common.Hash{}, err
// 	}
// 	if info := ss.burner.TracingInfo(); info != nil {
// 		info.RecordStorageGet(ss.slot)
// 	}
// 	return ss.db.GetState(ss.account, ss.slot), nil
// }

// func (ss *StorageSlot) Set(value common.Hash) error {
// 	if ss.burner.ReadOnly() {
// 		log.Error("Read-only burner attempted to mutate state", "value", value)
// 		return vm.ErrWriteProtection
// 	}
// 	err := ss.burner.Burn(multigas.ResourceKindStorageAccess, writeCost(value))
// 	if err != nil {
// 		return err
// 	}
// 	if info := ss.burner.TracingInfo(); info != nil {
// 		info.RecordStorageSet(ss.slot, value)
// 	}
// 	ss.db.SetState(ss.account, ss.slot, value)
// 	return nil
// }

// // StorageBackedInt64 is an int64 stored inside the StateDB.
// // Implementation note: Conversions between big.Int and common.Hash give weird results
// // for negative values, so we cast to uint64 before writing to storage and cast back to int64 after reading.
// // Golang casting between uint64 and int64 doesn't change the data, it just reinterprets the same 8 bytes,
// // so this is a hacky but reliable way to store an 8-byte int64 in a common.Hash storage slot.
// type StorageBackedInt64 struct {
// 	StorageSlot
// }

pub struct StorageBackedInt64<B: Burner>(StorageSlot<B>);

impl<B: Burner> StorageBackedInt64<B> {
    /// Reads the stored value and reinterprets its low 64 bits as `i64`.
    ///
    /// Maps to Go's `StorageBackedInt64.Get`. The value is stored as its
    /// two's-complement `uint64` bit pattern (see implementation note in the
    /// Go source: casting between `uint64` and `int64` reinterprets the same
    /// 8 bytes without changing them).
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<i64, Db::Error> {
        let raw = self.0.get(db)?;
        let as_u64 = u64::try_from(raw)
            .unwrap_or_else(|_| panic!("invalid value found in StorageBackedInt64 storage"));
        Ok(as_u64 as i64)
    }

    /// Writes `value` by reinterpreting its bits as `u64` before storing.
    ///
    /// Maps to Go's `StorageBackedInt64.Set`.
    pub fn set<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        value: i64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, StorageValue::from(value as u64))
    }
}

// func (s *Storage) OpenStorageBackedInt64(offset uint64) StorageBackedInt64 {
// 	return StorageBackedInt64{s.NewSlot(offset)}
// }

// func (sbu *StorageBackedInt64) Get() (int64, error) {
// 	raw, err := sbu.StorageSlot.Get()
// 	if !raw.Big().IsUint64() {
// 		panic("invalid value found in StorageBackedInt64 storage")
// 	}
// 	// #nosec G115
// 	return int64(raw.Big().Uint64()), err // see implementation note above
// }

// func (sbu *StorageBackedInt64) Set(value int64) error {
// 	// #nosec G115
// 	return sbu.StorageSlot.Set(util.UintToHash(uint64(value))) // see implementation note above
// }

// // StorageBackedBips represents a number of basis points
// type StorageBackedBips struct {
// 	backing StorageBackedInt64
// }

// func (s *Storage) OpenStorageBackedBips(offset uint64) StorageBackedBips {
// 	return StorageBackedBips{StorageBackedInt64{s.NewSlot(offset)}}
// }

// func (sbu *StorageBackedBips) Get() (arbmath.Bips, error) {
// 	value, err := sbu.backing.Get()
// 	return arbmath.Bips(value), err
// }

// func (sbu *StorageBackedBips) Set(bips arbmath.Bips) error {
// 	return sbu.backing.Set(int64(bips))
// }

// // StorageBackedUBips represents an unsigned number of basis points
// type StorageBackedUBips struct {
// 	backing StorageBackedUint64
// }

// func (s *Storage) OpenStorageBackedUBips(offset uint64) StorageBackedUBips {
// 	return StorageBackedUBips{StorageBackedUint64{s.NewSlot(offset)}}
// }

// func (sbu *StorageBackedUBips) Get() (arbmath.UBips, error) {
// 	value, err := sbu.backing.Get()
// 	return arbmath.UBips(value), err
// }

// func (sbu *StorageBackedUBips) Set(bips arbmath.UBips) error {
// 	return sbu.backing.Set(uint64(bips))
// }

// type StorageBackedUint16 struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedUint16(offset uint64) StorageBackedUint16 {
// 	return StorageBackedUint16{s.NewSlot(offset)}
// }

// func (sbu *StorageBackedUint16) Get() (uint16, error) {
// 	raw, err := sbu.StorageSlot.Get()
// 	big := raw.Big()
// 	if !big.IsUint64() || big.Uint64() > math.MaxUint16 {
// 		panic("expected uint16 compatible value in storage")
// 	}
// 	// #nosec G115
// 	return uint16(big.Uint64()), err
// }

// func (sbu *StorageBackedUint16) Set(value uint16) error {
// 	bigValue := new(big.Int).SetUint64(uint64(value))
// 	return sbu.StorageSlot.Set(common.BigToHash(bigValue))
// }

// type StorageBackedUint24 struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedUint24(offset uint64) StorageBackedUint24 {
// 	return StorageBackedUint24{s.NewSlot(offset)}
// }

// func (sbu *StorageBackedUint24) Get() (arbmath.Uint24, error) {
// 	raw, err := sbu.StorageSlot.Get()
// 	value := arbmath.BigToUint24OrPanic(raw.Big())
// 	return value, err
// }

// func (sbu *StorageBackedUint24) Set(value arbmath.Uint24) error {
// 	return sbu.StorageSlot.Set(common.BigToHash(value.ToBig()))
// }

// type StorageBackedUint32 struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedUint32(offset uint64) StorageBackedUint32 {
// 	return StorageBackedUint32{s.NewSlot(offset)}
// }

// func (sbu *StorageBackedUint32) Get() (uint32, error) {
// 	raw, err := sbu.StorageSlot.Get()
// 	big := raw.Big()
// 	if !big.IsUint64() || big.Uint64() > math.MaxUint32 {
// 		panic("expected uint32 compatible value in storage")
// 	}
// 	// #nosec G115
// 	return uint32(big.Uint64()), err
// }

// func (sbu *StorageBackedUint32) Set(value uint32) error {
// 	bigValue := new(big.Int).SetUint64(uint64(value))
// 	return sbu.StorageSlot.Set(common.BigToHash(bigValue))
// }

// func (sbu *StorageBackedUint32) Clear() error {
// 	return sbu.Set(0)
// }

pub struct StorageBackedUint64<B: Burner>(StorageSlot<B>);

impl<B: Burner> StorageBackedUint64<B> {
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        let raw = self.0.get(db)?;
        let as_u64 = u64::try_from(raw)
            .unwrap_or_else(|_| panic!("expected uint64 compatible value in storage"));
        Ok(as_u64)
    }

    pub fn set<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        value: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, StorageValue::from(value))
    }

    pub fn clear<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.set(ctx, 0)
    }

    pub fn increment<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<u64, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let old = self.get(ctx.db_mut())?;
        if old == u64::MAX {
            panic!("Overflow in StorageBackedUint64::Increment");
        }
        let new = old + 1;
        self.set(ctx, new)?;
        Ok(new)
    }

    pub fn decrement<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<u64, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let old = self.get(ctx.db_mut())?;
        if old == 0 {
            panic!("Underflow in StorageBackedUint64::Decrement");
        }
        let new = old - 1;
        self.set(ctx, new)?;
        Ok(new)
    }
}

// type StorageBackedUint64 struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedUint64(offset uint64) StorageBackedUint64 {
// 	return StorageBackedUint64{s.NewSlot(offset)}
// }

// func (sbu *StorageBackedUint64) Get() (uint64, error) {
// 	raw, err := sbu.StorageSlot.Get()
// 	if !raw.Big().IsUint64() {
// 		panic("expected uint64 compatible value in storage")
// 	}
// 	return raw.Big().Uint64(), err
// }

// func (sbu *StorageBackedUint64) Set(value uint64) error {
// 	bigValue := new(big.Int).SetUint64(value)
// 	return sbu.StorageSlot.Set(common.BigToHash(bigValue))
// }

// func (sbu *StorageBackedUint64) Clear() error {
// 	return sbu.Set(0)
// }

// func (sbu *StorageBackedUint64) Increment() (uint64, error) {
// 	old, err := sbu.Get()
// 	if err != nil {
// 		return 0, err
// 	}
// 	if old+1 < old {
// 		panic("Overflow in StorageBackedUint64::Increment")
// 	}
// 	return old + 1, sbu.Set(old + 1)
// }

// func (sbu *StorageBackedUint64) Decrement() (uint64, error) {
// 	old, err := sbu.Get()
// 	if err != nil {
// 		return 0, err
// 	}
// 	if old == 0 {
// 		panic("Underflow in StorageBackedUint64::Decrement")
// 	}
// 	return old - 1, sbu.Set(old - 1)
// }

// type MemoryBackedUint64 struct {
// 	contents uint64
// }

// func (mbu *MemoryBackedUint64) Get() (uint64, error) {
// 	return mbu.contents, nil
// }

// func (mbu *MemoryBackedUint64) Set(val uint64) error {
// 	mbu.contents = val
// 	return nil
// }

// func (mbu *MemoryBackedUint64) Increment() (uint64, error) {
// 	old := mbu.contents
// 	if old+1 < old {
// 		panic("Overflow in MemoryBackedUint64::Increment")
// 	}
// 	return old + 1, mbu.Set(old + 1)
// }

// func (mbu *MemoryBackedUint64) Decrement() (uint64, error) {
// 	old := mbu.contents
// 	if old == 0 {
// 		panic("Underflow in MemoryBackedUint64::Decrement")
// 	}
// 	return old - 1, mbu.Set(old - 1)
// }

// type WrappedUint64 interface {
// 	Get() (uint64, error)
// 	Set(uint64) error
// 	Increment() (uint64, error)
// 	Decrement() (uint64, error)
// }

// var twoToThe256 = new(big.Int).Lsh(common.Big1, 256)
// var twoToThe256MinusOne = new(big.Int).Sub(twoToThe256, common.Big1)
// var twoToThe255 = new(big.Int).Lsh(common.Big1, 255)
// var twoToThe255MinusOne = new(big.Int).Sub(twoToThe255, common.Big1)

pub struct StorageBackedBigUint<B: Burner>(StorageSlot<B>);

impl<B: Burner> StorageBackedBigUint<B> {
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<U256, Db::Error> {
        self.0.get(db)
    }

    /// Stores `val`, returning an error via the burner if `val` would underflow
    /// or overflow. In Go this is guarded against negative values and values
    /// wider than 256 bits; both are impossible with `U256`, so no runtime
    /// checks are needed here.
    pub fn set_checked<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: U256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, val)
    }

    /// Stores `val`, saturating to `[0, U256::MAX]` with a warning if the
    /// value is out of range. With `U256` the range is always satisfied, so
    /// this is equivalent to `set_checked`. The `name` parameter is kept for
    /// call-site parity with Go.
    pub fn set_saturating_with_warning<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: U256,
        _name: &str,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, val)
    }
}

// type StorageBackedBigUint struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedBigUint(offset uint64) StorageBackedBigUint {
// 	return StorageBackedBigUint{s.NewSlot(offset)}
// }

// func (sbbu *StorageBackedBigUint) Get() (*big.Int, error) {
// 	asHash, err := sbbu.StorageSlot.Get()
// 	if err != nil {
// 		return nil, err
// 	}
// 	return asHash.Big(), nil
// }

// // Warning: this will panic if it underflows or overflows with a system burner
// // SetSaturatingWithWarning is likely better
// func (sbbu *StorageBackedBigUint) SetChecked(val *big.Int) error {
// 	if val.Sign() < 0 {
// 		return sbbu.burner.HandleError(fmt.Errorf("underflow in StorageBackedBigUint.Set setting value %v", val))
// 	}
// 	if val.BitLen() > 256 {
// 		return sbbu.burner.HandleError(fmt.Errorf("overflow in StorageBackedBigUint.Set setting value %v", val))
// 	}
// 	return sbbu.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

// func (sbbu *StorageBackedBigUint) SetSaturatingWithWarning(val *big.Int, name string) error {
// 	if val.Sign() < 0 {
// 		log.Warn("ArbOS storage big uint underflowed", "name", name, "value", val)
// 		val = common.Big0
// 	} else if val.BitLen() > 256 {
// 		log.Warn("ArbOS storage big uint overflowed", "name", name, "value", val)
// 		val = twoToThe256MinusOne
// 	}
// 	return sbbu.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

pub struct StorageBackedBigInt<B: Burner>(StorageSlot<B>);

// type StorageBackedBigInt struct {
// 	StorageSlot
// }

// func (s *Storage) OpenStorageBackedBigInt(offset uint64) StorageBackedBigInt {
// 	return StorageBackedBigInt{s.NewSlot(offset)}
// }

impl<B: Burner> StorageBackedBigInt<B> {
    /// Reads the stored two's-complement signed 256-bit integer.
    ///
    /// Maps to Go's manual bit-255 sign check: `I256::from_raw` interprets the
    /// raw U256 bit pattern as two's complement, which is identical logic.
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        let raw = self.0.get(db)?;
        Ok(I256::from_raw(raw))
    }

    /// Stores `val`, returning a burner error on under/overflow.
    ///
    /// Go guards against `*big.Int` values outside [-2^255, 2^255-1]. `I256`
    /// is already constrained to that range, so no runtime check is needed.
    pub fn set_checked<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: I256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, val.into_raw())
    }

    /// Stores `val`, saturating to `[I256::MIN, I256::MAX]` with a log warning
    /// on out-of-range input. `I256` is already constrained to that range, so
    /// saturation is a no-op. `name` is kept for call-site parity with Go.
    pub fn set_saturating_with_warning<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: I256,
        _name: &str,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, val.into_raw())
    }

    /// Pre-version-7 storage format: stores the unsigned magnitude of `val`
    /// rather than its two's-complement bit pattern.
    ///
    /// Go's `big.Int.Bytes()` returns the absolute value as bytes, so negative
    /// values were stored as their magnitude (a bug fixed in version 7).
    /// `I256::unsigned_abs()` replicates that behaviour exactly.
    pub fn set_pre_version7<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: I256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, val.unsigned_abs())
    }

    /// Stores a non-negative value given as a raw `u64`.
    ///
    /// Maps to Go's `StorageBackedBigInt.SetByUint`.
    pub fn set_by_uint<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, StorageValue::from(val))
    }
}
// func (sbbi *StorageBackedBigInt) Get() (*big.Int, error) {
// 	asHash, err := sbbi.StorageSlot.Get()
// 	if err != nil {
// 		return nil, err
// 	}
// 	asBig := new(big.Int).SetBytes(asHash[:])
// 	if asBig.Bit(255) != 0 {
// 		asBig = new(big.Int).Sub(asBig, twoToThe256)
// 	}
// 	return asBig, err
// }

// // Warning: this will panic if it underflows or overflows with a system burner
// // SetSaturatingWithWarning is likely better
// func (sbbi *StorageBackedBigInt) SetChecked(val *big.Int) error {
// 	if val.Sign() < 0 {
// 		val = new(big.Int).Add(val, twoToThe256)
// 		if val.BitLen() < 256 || val.Sign() <= 0 { // require that it's positive and the top bit is set
// 			return sbbi.burner.HandleError(fmt.Errorf("underflow in StorageBackedBigInt.Set setting value %v", val))
// 		}
// 	} else if val.BitLen() >= 256 {
// 		return sbbi.burner.HandleError(fmt.Errorf("overflow in StorageBackedBigInt.Set setting value %v", val))
// 	}
// 	return sbbi.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

// func (sbbi *StorageBackedBigInt) SetSaturatingWithWarning(val *big.Int, name string) error {
// 	if val.Sign() < 0 {
// 		origVal := val
// 		val = new(big.Int).Add(val, twoToThe256)
// 		if val.BitLen() < 256 || val.Sign() <= 0 { // require that it's positive and the top bit is set
// 			log.Warn("ArbOS storage big uint underflowed", "name", name, "value", origVal)
// 			val.Set(twoToThe255)
// 		}
// 	} else if val.BitLen() >= 256 {
// 		log.Warn("ArbOS storage big uint overflowed", "name", name, "value", val)
// 		val = twoToThe255MinusOne
// 	}
// 	return sbbi.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

// func (sbbi *StorageBackedBigInt) Set_preVersion7(val *big.Int) error {
// 	return sbbi.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

// func (sbbi *StorageBackedBigInt) SetByUint(val uint64) error {
// 	return sbbi.StorageSlot.Set(util.UintToHash(val))
// }

// type StorageBackedAddress struct {
// 	StorageSlot
// }

pub struct StorageBackedAddress<B: Burner>(StorageSlot<B>);

impl<B: Burner> StorageBackedAddress<B> {
    /// Reads the address stored in the slot.
    ///
    /// Maps to Go's `common.BytesToAddress(value.Bytes())`: the address
    /// occupies the rightmost 20 bytes of the 32-byte slot (left-padded with
    /// 12 zero bytes per EVM convention).
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<Address, Db::Error> {
        let raw = self.0.get(db)?;
        Ok(Address::from_slice(&raw.to_be_bytes::<32>()[12..]))
    }

    /// Stores `val` left-padded with 12 zero bytes into the 32-byte slot.
    ///
    /// Maps to Go's `util.AddressToHash(val)`. `Address::into_word()` performs
    /// that same left-padding, producing a 32-byte big-endian value.
    pub fn set<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: Address,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set(ctx, U256::from_be_bytes(val.into_word().0))
    }
}

// func (s *Storage) OpenStorageBackedAddress(offset uint64) StorageBackedAddress {
// 	return StorageBackedAddress{s.NewSlot(offset)}
// }

// func (sba *StorageBackedAddress) Get() (common.Address, error) {
// 	value, err := sba.StorageSlot.Get()
// 	return common.BytesToAddress(value.Bytes()), err
// }

// func (sba *StorageBackedAddress) Set(val common.Address) error {
// 	return sba.StorageSlot.Set(util.AddressToHash(val))
// }

// type StorageBackedAddressOrNil struct {
// 	StorageSlot
// }

// var NilAddressRepresentation common.Hash

// func init() {
// 	NilAddressRepresentation = common.BigToHash(new(big.Int).Lsh(big.NewInt(1), 255))
// }

pub struct StorageBackedAddressOrNil<B: Burner>(StorageSlot<B>);

impl<B: Burner> StorageBackedAddressOrNil<B> {
    /// Sentinel value representing `None`: `1 << 255`.
    ///
    /// This can never be a valid address (addresses occupy only the low 20
    /// bytes of a slot, leaving the top 12 bytes zero), so it is safe to
    /// use as a distinct nil marker.
    ///
    /// Maps to Go's `NilAddressRepresentation = BigToHash(1 << 255)`.
    /// In little-endian U256 limbs, bit 255 is the MSB of limb[3].
    const NIL: U256 = U256::from_limbs([0, 0, 0, 1u64 << 63]);

    /// Returns `None` if the slot holds the nil sentinel, otherwise decodes
    /// the address from the low 20 bytes.
    ///
    /// Maps to Go's `StorageBackedAddressOrNil.Get` which returns `*common.Address`.
    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<Option<Address>, Db::Error> {
        let raw = self.0.get(db)?;
        if raw == Self::NIL {
            return Ok(None);
        }
        Ok(Some(Address::from_slice(&raw.to_be_bytes::<32>()[12..])))
    }

    /// Stores `None` as the nil sentinel, or left-pads a `Some` address into
    /// the slot.
    ///
    /// Maps to Go's `StorageBackedAddressOrNil.Set` which accepts `*common.Address`.
    pub fn set<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: Option<Address>,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let raw = match val {
            None => Self::NIL,
            Some(addr) => U256::from_be_bytes(addr.into_word().0),
        };
        self.0.set(ctx, raw)
    }
}

// func (s *Storage) OpenStorageBackedAddressOrNil(offset uint64) StorageBackedAddressOrNil {
// 	return StorageBackedAddressOrNil{s.NewSlot(offset)}
// }

// func (sba *StorageBackedAddressOrNil) Get() (*common.Address, error) {
// 	asHash, err := sba.StorageSlot.Get()
// 	if asHash == NilAddressRepresentation || err != nil {
// 		return nil, err
// 	} else {
// 		ret := common.BytesToAddress(asHash.Bytes())
// 		return &ret, nil
// 	}
// }

// func (sba *StorageBackedAddressOrNil) Set(val *common.Address) error {
// 	if val == nil {
// 		return sba.StorageSlot.Set(NilAddressRepresentation)
// 	}
// 	return sba.StorageSlot.Set(common.BytesToHash(val.Bytes()))
// }

/// A namespaced multi-slot EVM storage space, the direct analogue of Go's
/// `Storage` struct. All `StorageBacked*` byte-level types embed this.
///
/// Slots are addressed by hashing `storage_key ++ key[0..31]` and preserving
/// `key[31]` as the last byte (Go's `mapAddress`), so that 256 consecutive
/// logical slots share a common keccak prefix ("page").
pub struct Storage<B: Burner> {
    pub account: Address,
    /// Pre-computed keccak256 namespace key. Matches Go's `Storage.storageKey`
    /// after `OpenSubStorage`: `keccak256(parent.storageKey ++ id)`.
    pub storage_key: Vec<u8>,
    pub burner: B,
    // Go's `db vm.StateDB` is intentionally absent: revm threads the database
    // through each call site rather than storing it, so callers pass `db` or
    // `ctx` directly to read/write methods.
    //
    // Go's `hashCache *lru.Cache[string, []byte]` is not yet implemented.
    // It memoises `cachedKeccak` results to avoid rehashing the same
    // `storageKey ++ key[0..31]` inputs on every slot access. A Rust
    // equivalent would be an `Option<Arc<Mutex<LruCache<[u8; 31], [u8; 31]>>>>`
    // or similar.
}

impl<B: Burner> Storage<B> {
    /// Derives the physical EVM slot for logical slot `offset`.
    ///
    /// Maps to Go's `Storage.mapAddress(util.UintToHash(offset))`.
    pub fn map_address(&self, offset: u64) -> StorageKey {
        let key_bytes = U256::from(offset).to_be_bytes::<32>();
        const BOUNDARY: usize = 31;
        let mut input = Vec::with_capacity(self.storage_key.len() + BOUNDARY);
        input.extend_from_slice(&self.storage_key);
        input.extend_from_slice(&key_bytes[..BOUNDARY]);
        let hash = keccak256(&input);
        let mut mapped = [0u8; 32];
        mapped[..BOUNDARY].copy_from_slice(&hash.0[..BOUNDARY]);
        mapped[BOUNDARY] = key_bytes[BOUNDARY];
        StorageKey::from_be_bytes(mapped)
    }

    pub fn read_slot<Db: Database>(&self, db: &mut Db, offset: u64) -> Result<StorageValue, Db::Error> {
        db.storage(self.account, self.map_address(offset))
    }

    pub fn write_slot<CTX: ContextTr>(
        &self,
        ctx: &mut CTX,
        offset: u64,
        value: StorageValue,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        ctx.journal_mut()
            .sstore(self.account, self.map_address(offset), value)
            .map(|_| ())
    }

    /// Maps to Go's `Storage.GetBytesSize`.
    pub fn get_bytes_size<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        let raw = self.read_slot(db, 0)?;
        Ok(u64::try_from(raw)
            .unwrap_or_else(|_| panic!("invalid byte length in StorageBackedBytes")))
    }

    /// Maps to Go's `Storage.GetBytes`.
    pub fn get_bytes<Db: Database>(&self, db: &mut Db) -> Result<Vec<u8>, Db::Error> {
        let mut bytes_left = self.get_bytes_size(db)?;
        let mut result = Vec::with_capacity(bytes_left as usize);
        let mut offset = 1u64;
        while bytes_left >= 32 {
            let chunk = self.read_slot(db, offset)?.to_be_bytes::<32>();
            result.extend_from_slice(&chunk);
            bytes_left -= 32;
            offset += 1;
        }
        if bytes_left > 0 {
            let chunk = self.read_slot(db, offset)?.to_be_bytes::<32>();
            result.extend_from_slice(&chunk[32 - bytes_left as usize..]);
        }
        Ok(result)
    }

    /// Maps to Go's `Storage.ClearBytes`.
    pub fn clear_bytes<CTX: ContextTr>(
        &self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let mut bytes_left = {
            let raw = self.read_slot(ctx.db_mut(), 0)?;
            u64::try_from(raw)
                .unwrap_or_else(|_| panic!("invalid byte length in StorageBackedBytes"))
        };
        let mut offset = 1u64;
        while bytes_left > 0 {
            self.write_slot(ctx, offset, StorageValue::ZERO)?;
            offset += 1;
            bytes_left = bytes_left.saturating_sub(32);
        }
        self.write_slot(ctx, 0, StorageValue::ZERO)
    }

    /// Maps to Go's `Storage.SetBytes`.
    pub fn set_bytes<CTX: ContextTr>(
        &self,
        ctx: &mut CTX,
        b: &[u8],
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.clear_bytes(ctx)?;
        self.write_slot(ctx, 0, StorageValue::from(b.len() as u64))?;
        let mut offset = 1u64;
        let mut remaining = b;
        while remaining.len() >= 32 {
            let value = StorageValue::from_be_bytes::<32>(remaining[..32].try_into().unwrap());
            self.write_slot(ctx, offset, value)?;
            remaining = &remaining[32..];
            offset += 1;
        }
        if !remaining.is_empty() {
            let mut padded = [0u8; 32];
            padded[32 - remaining.len()..].copy_from_slice(remaining);
            self.write_slot(ctx, offset, StorageValue::from_be_bytes(padded))?;
        }
        Ok(())
    }
}

/// Variable-length bytes stored across multiple EVM slots.
///
/// Mirrors Go's `StorageBackedBytes` which embeds a full `Storage` (not a
/// single `StorageSlot`). Layout:
///   slot 0        : byte length as u64
///   slots 1..=n   : full 32-byte data chunks
///   slot n+1      : remaining bytes right-aligned (if length % 32 != 0)
pub struct StorageBackedBytes<B: Burner>(Storage<B>);

impl<B: Burner> StorageBackedBytes<B> {
    pub fn size<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.0.get_bytes_size(db)
    }

    pub fn get<Db: Database>(&self, db: &mut Db) -> Result<Vec<u8>, Db::Error> {
        self.0.get_bytes(db)
    }

    pub fn clear<CTX: ContextTr>(
        &self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.clear_bytes(ctx)
    }

    pub fn set<CTX: ContextTr>(
        &self,
        ctx: &mut CTX,
        b: &[u8],
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.0.set_bytes(ctx, b)
    }
}

// type StorageBackedBytes struct {
// 	Storage
// }

// func (s *Storage) OpenStorageBackedBytes(id []byte) StorageBackedBytes {
// 	return StorageBackedBytes{
// 		*s.OpenSubStorage(id),
// 	}
// }

// func (sbb *StorageBackedBytes) Get() ([]byte, error) {
// 	return sbb.Storage.GetBytes()
// }

// func (sbb *StorageBackedBytes) Set(val []byte) error {
// 	return sbb.Storage.SetBytes(val)
// }

// func (sbb *StorageBackedBytes) Clear() error {
// 	return sbb.Storage.ClearBytes()
// }

// func (sbb *StorageBackedBytes) Size() (uint64, error) {
// 	return sbb.Storage.GetBytesSize()
// }
