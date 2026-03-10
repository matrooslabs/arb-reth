// type ArbosState struct {
// 	arbosVersion                    uint64                      // version of the ArbOS storage format and semantics
// 	upgradeVersion                  storage.StorageBackedUint64 // version we're planning to upgrade to, or 0 if not planning to upgrade
// 	upgradeTimestamp                storage.StorageBackedUint64 // when to do the planned upgrade
// 	networkFeeAccount               storage.StorageBackedAddress
// 	l1PricingState                  *l1pricing.L1PricingState
// 	l2PricingState                  *l2pricing.L2PricingState
// 	retryableState                  *retryables.RetryableState
// 	addressTable                    *addressTable.AddressTable
// 	chainOwners                     *addressSet.AddressSet
// 	nativeTokenOwners               *addressSet.AddressSet
// 	transactionFilterers            *addressSet.AddressSet
// 	filteredTransactions            *filteredTransactions.FilteredTransactionsState
// 	sendMerkle                      *merkleAccumulator.MerkleAccumulator
// 	programs                        *programs.Programs
// 	features                        *features.Features
// 	blockhashes                     *blockhash.Blockhashes
// 	chainId                         storage.StorageBackedBigInt
// 	chainConfig                     storage.StorageBackedBytes
// 	genesisBlockNum                 storage.StorageBackedUint64
// 	infraFeeAccount                 storage.StorageBackedAddress
// 	brotliCompressionLevel          storage.StorageBackedUint64 // brotli compression level used for pricing
// 	nativeTokenEnabledTime          storage.StorageBackedUint64
// 	transactionFilteringEnabledTime storage.StorageBackedUint64
// 	filteredFundsRecipient          storage.StorageBackedAddress
// 	backingStorage                  *storage.Storage
// 	Burner                          burn.Burner
// }

use crate::{
    burn::Burner, l1pricing::l1pricing::L1PricingState, l2pricing::l2pricing::L2PricingState, storage::storage::{StorageBackedAddress, StorageBackedUint64}
};

pub struct ArbosState<B: Burner> {
    arbos_version: u64,
    upgrade_version: StorageBackedUint64<B>,
    upgrade_timestamp: StorageBackedUint64<B>,
    network_fee_account: StorageBackedAddress<B>,
    l1_pricing_state: L1PricingState<B>,
    l2_pricing_state: L2PricingState<B>,
}
