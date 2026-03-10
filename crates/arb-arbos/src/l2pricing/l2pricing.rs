use crate::{
    burn::Burner,
    storage::{
        storage::{Storage, StorageBackedBigUint, StorageBackedUint64},
        vector::SubStorageVector,
    },
};

pub struct L2PricingState<B: Burner> {
    storage: Storage<B>,
    speed_limit_per_second: StorageBackedUint64<B>,
    per_block_gas_limit: StorageBackedUint64<B>,
    base_fee_wei: StorageBackedBigUint<B>,
    min_base_fee_wei: StorageBackedBigUint<B>,
    gas_backlog: StorageBackedUint64<B>,
    pricing_inertia: StorageBackedUint64<B>,
    backlog_tolerance: StorageBackedUint64<B>,
    per_tx_gas_limit: StorageBackedUint64<B>,
    gas_constraints: SubStorageVector<B>,
    multi_gas_constraints: SubStorageVector<B>,
    // multiGasFees: SubStorageVector / MultiGasFees not yet ported
    arbos_version: u64,
}

// type L2PricingState struct {
// 	storage             *storage.Storage
// 	speedLimitPerSecond storage.StorageBackedUint64
// 	perBlockGasLimit    storage.StorageBackedUint64
// 	baseFeeWei          storage.StorageBackedBigUint
// 	minBaseFeeWei       storage.StorageBackedBigUint
// 	gasBacklog          storage.StorageBackedUint64
// 	pricingInertia      storage.StorageBackedUint64
// 	backlogTolerance    storage.StorageBackedUint64
// 	perTxGasLimit       storage.StorageBackedUint64
// 	gasConstraints      *storage.SubStorageVector
// 	multiGasConstraints *storage.SubStorageVector
// 	multiGasFees        *MultiGasFees

// 	ArbosVersion uint64
// }
