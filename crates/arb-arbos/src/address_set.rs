// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package addressSet

// // TODO lowercase this package name

// import (
// 	"errors"

// 	"github.com/ethereum/go-ethereum/common"
// 	"github.com/ethereum/go-ethereum/params"

// 	"github.com/offchainlabs/nitro/arbos/storage"
// 	"github.com/offchainlabs/nitro/arbos/util"
// )

// // AddressSet represents a set of addresses
// // size is stored at position 0
// // members of the set are stored sequentially from 1 onward
// type AddressSet struct {
// 	backingStorage *storage.Storage
// 	size           storage.StorageBackedUint64
// 	byAddress      *storage.Storage
// }

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{Address, StorageKey, StorageValue},
};

use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedUint64},
};

pub struct AddressSet<B: Burner> {
    backing_storage: Storage<B>,
    size: StorageBackedUint64<B>,
    by_address: Storage<B>,
}

impl<B: Burner> AddressSet<B> {
    /// Opens an `AddressSet` backed by `sto`.
    ///
    /// Maps to Go's `OpenAddressSet`.
    pub fn new(sto: Storage<B>) -> Self
    where
        B: Clone,
    {
        let size = sto.open_storage_backed_uint64(0);
        let by_address = sto.open_sub_storage(&[0u8]);
        AddressSet { backing_storage: sto, size, by_address }
    }

    /// Writes the initial size of zero into slot 0. Call once on a fresh storage
    /// **before** opening the `AddressSet` via `new`.
    ///
    /// Maps to Go's package-level `Initialize(sto *storage.Storage)` — a free
    /// function that operates on the raw storage before it is wrapped.
    // func Initialize(sto *storage.Storage) error {
    //     return sto.SetUint64ByUint64(0, 0)
    // }
    pub fn initialize<CTX: ContextTr>(
        sto: &mut Storage<B>,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error>
    where
        B: Clone,
    {
        sto.set_uint64_by_uint64(ctx, 0, 0)
    }
    

    /// Returns the number of members.
    ///
    /// Maps to Go's `AddressSet.Size`.
    pub fn size<Db: Database>(&mut self, db: &mut Db) -> Result<u64, Db::Error> {
        self.size.get(db)
    }

    /// Returns `true` if `addr` is in the set.
    ///
    /// Maps to Go's `AddressSet.IsMember`.
    pub fn is_member<Db: Database>(&self, db: &mut Db, addr: Address) -> Result<bool, Db::Error> {
        let key = addr_to_key(addr);
        let value = self.by_address.get_by_key(db, key)?;
        Ok(!value.is_zero())
    }

    /// Returns an arbitrary member, or `None` if the set is empty.
    ///
    /// Maps to Go's `AddressSet.GetAnyMember`.
    pub fn get_any_member<Db: Database>(&mut self, db: &mut Db) -> Result<Option<Address>, Db::Error> {
        if self.size.get(db)? == 0 {
            return Ok(None);
        }
        let raw = self.backing_storage.read_slot(db, 1)?;
        if raw.is_zero() {
            return Ok(None);
        }
        Ok(Some(raw_to_addr(raw)))
    }

    /// Removes all members, clearing both the sequential list and the address→slot mapping.
    ///
    /// Maps to Go's `AddressSet.Clear`.
    pub fn clear<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let sz = self.size.get(ctx.db_mut())?;
        if sz == 0 {
            return Ok(());
        }
        for i in 1..=sz {
            let contents = self.backing_storage.read_slot(ctx.db_mut(), i)?;
            self.backing_storage.write_slot(ctx, i, StorageValue::ZERO)?;
            self.by_address.clear_by_key(ctx, contents)?;
        }
        self.size.clear(ctx)
    }

    /// Returns up to `max` members in list order.
    ///
    /// Maps to Go's `AddressSet.AllMembers`.
    pub fn all_members<Db: Database>(&mut self, db: &mut Db, max: u64) -> Result<Vec<Address>, Db::Error> {
        let sz = self.size.get(db)?;
        let count = sz.min(max);
        let mut members = Vec::with_capacity(count as usize);
        for i in 0..count {
            let raw = self.backing_storage.read_slot(db, i + 1)?;
            members.push(raw_to_addr(raw));
        }
        Ok(members)
    }

    /// Clears the sequential list and resets size, but leaves the address→slot mapping intact.
    ///
    /// Maps to Go's `AddressSet.ClearList`.
    pub fn clear_list<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let sz = self.size.get(ctx.db_mut())?;
        if sz == 0 {
            return Ok(());
        }
        for i in 1..=sz {
            self.backing_storage.write_slot(ctx, i, StorageValue::ZERO)?;
        }
        self.size.clear(ctx)
    }

    /// Repairs a corrupted address→slot mapping entry for `addr`.
    ///
    /// Maps to Go's `AddressSet.RectifyMapping`.
    pub fn rectify_mapping<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        addr: Address,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        if !self.is_member(ctx.db_mut(), addr)? {
            panic!("RectifyMapping: Address is not an owner");
        }
        let addr_as_key = addr_to_key(addr);
        let slot = self.by_address.get_uint64_by_key(ctx.db_mut(), addr_as_key)?;
        let at_slot = self.backing_storage.read_slot(ctx.db_mut(), slot)?;
        let sz = self.size.get(ctx.db_mut())?;
        let addr_as_value = addr_to_value(addr);
        if at_slot == addr_as_value && slot <= sz {
            panic!("RectifyMapping: Owner address is correctly mapped");
        }
        self.by_address.clear_by_key(ctx, addr_as_key)?;
        self.add(ctx, addr)
    }

    /// Adds `addr` to the set. No-op if already a member.
    ///
    /// Maps to Go's `AddressSet.Add`.
    pub fn add<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        addr: Address,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        if self.is_member(ctx.db_mut(), addr)? {
            return Ok(());
        }
        let sz = self.size.get(ctx.db_mut())?;
        let slot = sz + 1;
        let addr_as_key = addr_to_key(addr);
        self.by_address.set_by_key(ctx, addr_as_key, StorageValue::from(slot))?;
        self.backing_storage.write_slot(ctx, slot, addr_to_value(addr))?;
        self.size.increment(ctx)?;
        Ok(())
    }

    /// Removes `addr` from the set. No-op if not a member.
    ///
    /// Maps to Go's `AddressSet.Remove`.
    pub fn remove<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        addr: Address,
        arbos_version: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let addr_as_key = addr_to_key(addr);
        let slot = self.by_address.get_uint64_by_key(ctx.db_mut(), addr_as_key)?;
        if slot == 0 {
            return Ok(());
        }
        self.by_address.clear_by_key(ctx, addr_as_key)?;
        let sz = self.size.get(ctx.db_mut())?;
        if slot < sz {
            let at_size = self.backing_storage.read_slot(ctx.db_mut(), sz)?;
            self.backing_storage.write_slot(ctx, slot, at_size)?;
            if arbos_version >= 11 {
                self.by_address.set_by_key(ctx, at_size, StorageValue::from(slot))?;
            }
        }
        self.backing_storage.write_slot(ctx, sz, StorageValue::ZERO)?;
        self.size.decrement(ctx)?;
        Ok(())
    }
}

/// Encodes an `Address` as a `StorageKey` (left-padded to 32 bytes).
///
/// Matches Go's `common.BytesToHash(addr.Bytes())` / `util.AddressToHash(addr)`.
#[inline]
fn addr_to_key(addr: Address) -> StorageKey {
    let mut bytes = [0u8; 32];
    bytes[12..].copy_from_slice(addr.as_slice());
    StorageKey::from_be_bytes(bytes)
}

/// Encodes an `Address` as a `StorageValue` (same encoding as `addr_to_key`).
///
/// Matches Go's `StorageBackedAddress.Set` — address stored in the low 20 bytes.
#[inline]
fn addr_to_value(addr: Address) -> StorageValue {
    addr_to_key(addr)
}

/// Decodes a raw slot value back to an `Address` (takes low 20 bytes).
///
/// Matches Go's `common.BytesToAddress(hash.Bytes())`.
#[inline]
fn raw_to_addr(raw: StorageValue) -> Address {
    Address::from_slice(&raw.to_be_bytes::<32>()[12..])
}



// func Initialize(sto *storage.Storage) error {
// 	return sto.SetUint64ByUint64(0, 0)
// }

// func OpenAddressSet(sto *storage.Storage) *AddressSet {
// 	return &AddressSet{
// 		backingStorage: sto.WithoutCache(),
// 		size:           sto.OpenStorageBackedUint64(0),
// 		byAddress:      sto.OpenSubStorage([]byte{0}),
// 	}
// }

// func (as *AddressSet) Size() (uint64, error) {
// 	return as.size.Get()
// }

// func (as *AddressSet) IsMember(addr common.Address) (bool, error) {
// 	value, err := as.byAddress.Get(util.AddressToHash(addr))
// 	return value != (common.Hash{}), err
// }

// func (as *AddressSet) GetAnyMember() (*common.Address, error) {
// 	size, err := as.size.Get()
// 	if err != nil || size == 0 {
// 		return nil, err
// 	}
// 	sba := as.backingStorage.OpenStorageBackedAddressOrNil(1)
// 	addr, err := sba.Get()
// 	return addr, err
// }

// func (as *AddressSet) Clear() error {
// 	size, err := as.size.Get()
// 	if err != nil || size == 0 {
// 		return err
// 	}
// 	for i := uint64(1); i <= size; i++ {
// 		contents, _ := as.backingStorage.GetByUint64(i)
// 		_ = as.backingStorage.ClearByUint64(i)
// 		err = as.byAddress.Clear(contents)
// 		if err != nil {
// 			return err
// 		}
// 	}
// 	return as.size.Clear()
// }

// func (as *AddressSet) AllMembers(maxNumToReturn uint64) ([]common.Address, error) {
// 	size, err := as.size.Get()
// 	if err != nil {
// 		return nil, err
// 	}
// 	if size > maxNumToReturn {
// 		size = maxNumToReturn
// 	}
// 	ret := make([]common.Address, size)
// 	for i := range ret {
// 		// #nosec G115
// 		sba := as.backingStorage.OpenStorageBackedAddress(uint64(i + 1))
// 		ret[i], err = sba.Get()
// 		if err != nil {
// 			return nil, err
// 		}
// 	}
// 	return ret, nil
// }

// func (as *AddressSet) ClearList() error {
// 	size, err := as.size.Get()
// 	if err != nil || size == 0 {
// 		return err
// 	}
// 	for i := uint64(1); i <= size; i++ {
// 		err = as.backingStorage.ClearByUint64(i)
// 		if err != nil {
// 			return err
// 		}
// 	}
// 	return as.size.Clear()
// }

// func (as *AddressSet) RectifyMapping(addr common.Address) error {
// 	isOwner, err := as.IsMember(addr)
// 	if !isOwner || err != nil {
// 		return errors.New("RectifyMapping: Address is not an owner")
// 	}

// 	// If the mapping is correct, RectifyMapping shouldn't do anything
// 	// Additional safety check to avoid corruption of mapping after the initial fix
// 	addrAsHash := common.BytesToHash(addr.Bytes())
// 	slot, err := as.byAddress.GetUint64(addrAsHash)
// 	if err != nil {
// 		return err
// 	}
// 	atSlot, err := as.backingStorage.GetByUint64(slot)
// 	if err != nil {
// 		return err
// 	}
// 	size, err := as.size.Get()
// 	if err != nil {
// 		return err
// 	}
// 	if atSlot == addrAsHash && slot <= size {
// 		return errors.New("RectifyMapping: Owner address is correctly mapped")
// 	}

// 	// Remove the owner from map and add them as a new owner
// 	err = as.byAddress.Clear(addrAsHash)
// 	if err != nil {
// 		return err
// 	}

// 	return as.Add(addr)
// }

// func (as *AddressSet) Add(addr common.Address) error {
// 	present, err := as.IsMember(addr)
// 	if present || err != nil {
// 		return err
// 	}
// 	size, err := as.size.Get()
// 	if err != nil {
// 		return err
// 	}
// 	slot := util.UintToHash(1 + size)
// 	addrAsHash := common.BytesToHash(addr.Bytes())
// 	err = as.byAddress.Set(addrAsHash, slot)
// 	if err != nil {
// 		return err
// 	}
// 	sba := as.backingStorage.OpenStorageBackedAddress(1 + size)
// 	err = sba.Set(addr)
// 	if err != nil {
// 		return err
// 	}
// 	_, err = as.size.Increment()
// 	return err
// }

// func (as *AddressSet) Remove(addr common.Address, arbosVersion uint64) error {
// 	addrAsHash := common.BytesToHash(addr.Bytes())
// 	slot, err := as.byAddress.GetUint64(addrAsHash)
// 	if slot == 0 || err != nil {
// 		return err
// 	}
// 	err = as.byAddress.Clear(addrAsHash)
// 	if err != nil {
// 		return err
// 	}
// 	size, err := as.size.Get()
// 	if err != nil {
// 		return err
// 	}
// 	if slot < size {
// 		atSize, err := as.backingStorage.GetByUint64(size)
// 		if err != nil {
// 			return err
// 		}
// 		err = as.backingStorage.SetByUint64(slot, atSize)
// 		if err != nil {
// 			return err
// 		}
// 		if arbosVersion >= params.ArbosVersion_11 {
// 			err = as.byAddress.Set(atSize, util.UintToHash(slot))
// 			if err != nil {
// 				return err
// 			}
// 		}
// 	}
// 	err = as.backingStorage.ClearByUint64(size)
// 	if err != nil {
// 		return err
// 	}
// 	_, err = as.size.Decrement()
// 	return err
// }
