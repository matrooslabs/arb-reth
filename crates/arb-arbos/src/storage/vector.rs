use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
};
use crate::{
    burn::Burner,
    storage::storage::{Storage, StorageBackedUint64},
};

const SUB_STORAGE_VECTOR_LENGTH_OFFSET: u64 = 0;

pub struct SubStorageVector<B: Burner> {
    storage: Storage<B>,
    length: StorageBackedUint64<B>,
}

impl<B: Burner> SubStorageVector<B> {
    // func OpenSubStorageVector(sto *Storage) *SubStorageVector
    pub fn open(sto: &Storage<B>) -> Self
    where
        B: Clone,
    {
        SubStorageVector {
            storage: Storage {
                account: sto.account,
                storage_key: sto.storage_key.clone(),
                burner: sto.burner.clone(),
                hash_cache: None,
            },
            length: sto.open_storage_backed_uint64(SUB_STORAGE_VECTOR_LENGTH_OFFSET),
        }
    }

    // func (v *SubStorageVector) Length() (uint64, error)
    pub fn length<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.length.get(db)
    }

    // func (v *SubStorageVector) Push() (*Storage, error)
    pub fn push<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<Storage<B>, <<CTX::Journal as JournalTr>::Database as Database>::Error>
    where
        B: Clone,
    {
        let length = self.length.get(ctx.db_mut())?;
        let id = length.to_be_bytes();
        let sub_storage = self.storage.open_sub_storage(&id);
        self.length.set(ctx, length + 1)?;
        Ok(sub_storage)
    }

    // func (v *SubStorageVector) Pop() (*Storage, error)
    pub fn pop<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
    ) -> Result<Storage<B>, <<CTX::Journal as JournalTr>::Database as Database>::Error>
    where
        B: Clone,
    {
        let length = self.length.get(ctx.db_mut())?;
        if length == 0 {
            panic!("sub-storage vector: can't pop empty");
        }
        let id = (length - 1).to_be_bytes();
        let sub_storage = self.storage.open_sub_storage(&id);
        self.length.set(ctx, length - 1)?;
        Ok(sub_storage)
    }

    // func (v *SubStorageVector) At(i uint64) *Storage
    // NOTE: does not verify out-of-bounds.
    pub fn at(&self, i: u64) -> Storage<B>
    where
        B: Clone,
    {
        let id = i.to_be_bytes();
        self.storage.open_sub_storage(&id)
    }
}

// // OpenSubStorageVector creates a SubStorageVector in given the root storage.
// func OpenSubStorageVector(sto *Storage) *SubStorageVector {
// 	return &SubStorageVector{
// 		sto.WithoutCache(),
// 		sto.OpenStorageBackedUint64(subStorageVectorLengthOffset),
// 	}
// }

// // Length returns the number of sub-storages.
// func (v *SubStorageVector) Length() (uint64, error) {
// 	length, err := v.length.Get()
// 	if err != nil {
// 		return 0, err
// 	}
// 	return length, err
// }

// // Push adds a new sub-storage at the end of the vector and return it.
// func (v *SubStorageVector) Push() (*Storage, error) {
// 	length, err := v.length.Get()
// 	if err != nil {
// 		return nil, err
// 	}
// 	id := binary.BigEndian.AppendUint64(nil, length)
// 	subStorage := v.storage.OpenSubStorage(id)
// 	if err := v.length.Set(length + 1); err != nil {
// 		return nil, err
// 	}
// 	return subStorage, nil
// }

// // Pop removes the last sub-storage from the end of the vector and return it.
// func (v *SubStorageVector) Pop() (*Storage, error) {
// 	length, err := v.length.Get()
// 	if err != nil {
// 		return nil, err
// 	}
// 	if length == 0 {
// 		return nil, errors.New("sub-storage vector: can't pop empty")
// 	}
// 	id := binary.BigEndian.AppendUint64(nil, length-1)
// 	subStorage := v.storage.OpenSubStorage(id)
// 	if err := v.length.Set(length - 1); err != nil {
// 		return nil, err
// 	}
// 	return subStorage, nil
// }

// // At returns the substorage at the given index.
// // NOTE: This function does not verify out-of-bounds.
// func (v *SubStorageVector) At(i uint64) *Storage {
// 	id := binary.BigEndian.AppendUint64(nil, i)
// 	subStorage := v.storage.OpenSubStorage(id)
// 	return subStorage
// }
// //
