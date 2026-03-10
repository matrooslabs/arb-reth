// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package l1pricing

// import (
// 	"encoding/binary"
// 	"errors"
// 	"fmt"
// 	"math/big"

// 	"github.com/ethereum/go-ethereum/common"
// 	"github.com/ethereum/go-ethereum/core"
// 	"github.com/ethereum/go-ethereum/core/tracing"
// 	"github.com/ethereum/go-ethereum/core/types"
// 	"github.com/ethereum/go-ethereum/core/vm"
// 	"github.com/ethereum/go-ethereum/crypto"
// 	"github.com/ethereum/go-ethereum/params"

// 	"github.com/offchainlabs/nitro/arbcompress"
// 	"github.com/offchainlabs/nitro/arbos/storage"
// 	"github.com/offchainlabs/nitro/arbos/util"
// 	"github.com/offchainlabs/nitro/cmd/chaininfo"
// 	"github.com/offchainlabs/nitro/util/arbmath"
// )

use alloy_primitives::{address, keccak256, B256};
use std::sync::LazyLock;
use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{Address, I256, U256},
};

// params.TxDataNonZeroGasEIP2028 in Go
const TX_DATA_NON_ZERO_GAS_EIP2028: u64 = 16;

// Arbitrum-internal tx type bytes (go-ethereum/core/types)
const ARB_DEPOSIT_TX_TYPE: u8 = 100;
const ARB_UNSIGNED_TX_TYPE: u8 = 101;
const ARB_CONTRACT_TX_TYPE: u8 = 102;
const ARB_RETRY_TX_TYPE: u8 = 104;
const ARB_SUBMIT_RETRYABLE_TX_TYPE: u8 = 105;
const ARB_INTERNAL_TX_TYPE: u8 = 106;
const ARB_LEGACY_TX_TYPE: u8 = 120;

/// Minimal transaction interface needed for L1 poster cost computation.
/// Callers implement this for their concrete transaction type.
pub trait PosterTransaction {
    /// RLP-encode the full transaction (Go: tx.MarshalBinary()).
    fn encode(&self) -> Vec<u8>;
    /// EIP-2718 transaction type byte (Go: tx.Type()).
    fn tx_type(&self) -> u8;
    /// Return cached calldata units for the given compression level, if present
    /// (Go: tx.GetCachedCalldataUnits(brotliCompressionLevel)).
    fn get_cached_calldata_units(&self, brotli_compression_level: u64) -> Option<u64>;
    /// Store calldata units for the given compression level in the transaction cache
    /// (Go: tx.SetCachedCalldataUnits(brotliCompressionLevel, units)).
    fn set_cached_calldata_units(&mut self, brotli_compression_level: u64, units: u64);
}

/// util.TxTypeHasPosterCosts in Go: returns false for Arbitrum-internal tx types.
fn tx_type_has_poster_costs(tx_type: u8) -> bool {
    !matches!(
        tx_type,
        ARB_DEPOSIT_TX_TYPE
            | ARB_UNSIGNED_TX_TYPE
            | ARB_CONTRACT_TX_TYPE
            | ARB_RETRY_TX_TYPE
            | ARB_SUBMIT_RETRYABLE_TX_TYPE
            | ARB_INTERNAL_TX_TYPE
            | ARB_LEGACY_TX_TYPE
    )
}

/// arbcompress.CompressLevel + len in Go: brotli-compress `input` at `level` (0–11)
/// and return the compressed byte count.
fn byte_count_after_brotli_level(input: &[u8], level: u64) -> u64 {
    use std::io::Write as _;
    let quality = level.min(11) as u32;
    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, quality, 22);
        writer.write_all(input).expect("brotli compression failed");
    }
    compressed.len() as u64
}

// ==== Random constants for fake tx (matching Go var declarations) ====

// var randomNonce = binary.BigEndian.Uint64(crypto.Keccak256([]byte("Nonce"))[:8])
static RANDOM_NONCE: LazyLock<u64> =
    LazyLock::new(|| u64::from_be_bytes(keccak256(b"Nonce")[..8].try_into().unwrap()));

// var randomGasTipCap = new(big.Int).SetBytes(crypto.Keccak256([]byte("GasTipCap"))[:4])
static RANDOM_GAS_TIP_CAP: LazyLock<U256> =
    LazyLock::new(|| U256::from_be_slice(&keccak256(b"GasTipCap")[..4]));

// var randomGasFeeCap = new(big.Int).SetBytes(crypto.Keccak256([]byte("GasFeeCap"))[:4])
static RANDOM_GAS_FEE_CAP: LazyLock<U256> =
    LazyLock::new(|| U256::from_be_slice(&keccak256(b"GasFeeCap")[..4]));

// var RandomGas = uint64(binary.BigEndian.Uint32(crypto.Keccak256([]byte("Gas"))[:4]))
static RANDOM_GAS: LazyLock<u64> =
    LazyLock::new(|| u32::from_be_bytes(keccak256(b"Gas")[..4].try_into().unwrap()) as u64);

// var randV = arbmath.BigMulByUint(chaininfo.ArbitrumOneChainConfig().ChainID, 3)
// Arbitrum One chain ID = 42161; 42161 * 3 = 126483.
const RAND_V: U256 = U256::from_limbs([126483, 0, 0, 0]);

// var randR = crypto.Keccak256Hash([]byte("R")).Big()
static RAND_R: LazyLock<U256> = LazyLock::new(|| U256::from_be_bytes(keccak256(b"R").0));

// var randS = crypto.Keccak256Hash([]byte("S")).Big()
static RAND_S: LazyLock<U256> = LazyLock::new(|| U256::from_be_bytes(keccak256(b"S").0));

// estimationPaddingUnits = 16 * params.TxDataNonZeroGasEIP2028 = 16 * 16 = 256
const ESTIMATION_PADDING_UNITS: u64 = TX_DATA_NON_ZERO_GAS_EIP2028 * TX_DATA_NON_ZERO_GAS_EIP2028;

// const estimationPaddingBasisPoints = 100  (bips above OneInBips = 10_000)
const ESTIMATION_PADDING_BASIS_POINTS: u64 = 100;

/// Simplified go-ethereum `core.Message`, with the fields needed for L1 poster cost.
pub struct Message {
    pub nonce: u64,
    pub gas_tip_cap: U256,
    pub gas_fee_cap: U256,
    pub gas_limit: u64,
    pub is_gas_estimation: bool,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub access_list: Vec<(Address, Vec<B256>)>,
    /// Go: message.Tx — the underlying transaction, if present; None during gas estimation.
    pub tx: Option<Box<dyn PosterTransaction>>,
}

/// A fake EIP-1559 transaction for gas estimation size purposes.
/// The tx is deliberately invalid — it is only fed to brotli for a size estimate.
pub struct FakeTx {
    encoded: Vec<u8>,
}

impl PosterTransaction for FakeTx {
    fn encode(&self) -> Vec<u8> {
        self.encoded.clone()
    }
    fn tx_type(&self) -> u8 {
        2 // EIP-1559
    }
    fn get_cached_calldata_units(&self, _brotli_compression_level: u64) -> Option<u64> {
        None // FakeTx is ephemeral; no caching needed
    }
    fn set_cached_calldata_units(&mut self, _brotli_compression_level: u64, _units: u64) {}
}

/// Allow `Box<dyn PosterTransaction>` to be used wherever `T: PosterTransaction` is required.
impl PosterTransaction for Box<dyn PosterTransaction> {
    fn encode(&self) -> Vec<u8> {
        (**self).encode()
    }
    fn tx_type(&self) -> u8 {
        (**self).tx_type()
    }
    fn get_cached_calldata_units(&self, level: u64) -> Option<u64> {
        (**self).get_cached_calldata_units(level)
    }
    fn set_cached_calldata_units(&mut self, level: u64, units: u64) {
        (**self).set_cached_calldata_units(level, units)
    }
}

/// Encode an Ethereum access list as RLP: a list of `[address, [storage_keys...]]` tuples.
fn encode_access_list(list: &[(Address, Vec<B256>)], out: &mut Vec<u8>) {
    use alloy_rlp::{Encodable, Header};
    let mut list_payload: Vec<u8> = Vec::new();
    for (addr, keys) in list {
        let mut entry_payload: Vec<u8> = Vec::new();
        addr.encode(&mut entry_payload);
        let mut keys_payload: Vec<u8> = Vec::new();
        for key in keys {
            key.encode(&mut keys_payload);
        }
        Header { list: true, payload_length: keys_payload.len() }.encode(&mut entry_payload);
        entry_payload.extend_from_slice(&keys_payload);
        Header { list: true, payload_length: entry_payload.len() }.encode(&mut list_payload);
        list_payload.extend_from_slice(&entry_payload);
    }
    Header { list: true, payload_length: list_payload.len() }.encode(out);
    out.extend_from_slice(&list_payload);
}

/// Build the EIP-2718 binary for a fake DynamicFeeTx (type 2). chain_id is 0
/// (not set on the fake tx struct in Go). randV/R/S form an invalid signature.
fn fake_eip1559_bytes(
    nonce: u64,
    gas_tip_cap: U256,
    gas_fee_cap: U256,
    gas: u64,
    to: Option<Address>,
    value: U256,
    data: &[u8],
    access_list: &[(Address, Vec<B256>)],
) -> Vec<u8> {
    use alloy_rlp::{Encodable, Header};
    let mut payload: Vec<u8> = Vec::new();
    0u64.encode(&mut payload);        // chain_id = 0
    nonce.encode(&mut payload);
    gas_tip_cap.encode(&mut payload);
    gas_fee_cap.encode(&mut payload);
    gas.encode(&mut payload);
    // to: None (contract creation) → RLP empty string; Some(addr) → 20-byte string.
    match to {
        None => (&[] as &[u8]).encode(&mut payload),
        Some(addr) => addr.encode(&mut payload),
    }
    value.encode(&mut payload);
    data.encode(&mut payload);
    encode_access_list(access_list, &mut payload);
    RAND_V.encode(&mut payload);    // fake y-parity / v  (const U256)
    (*RAND_R).encode(&mut payload); // fake r
    (*RAND_S).encode(&mut payload); // fake s

    let mut out = vec![0x02u8]; // EIP-2718 type prefix for EIP-1559
    Header { list: true, payload_length: payload.len() }.encode(&mut out);
    out.extend_from_slice(&payload);
    out
}

// // The returned tx will be invalid, likely for a number of reasons such as an invalid signature.
// // It's only used to check how large it is after brotli level 0 compression.
// func makeFakeTxForMessage(message *core.Message) *types.Transaction { ... }
pub fn make_fake_tx_for_message(message: &Message) -> FakeTx {
    let nonce = if message.nonce == 0 { *RANDOM_NONCE } else { message.nonce };
    let gas_tip_cap =
        if message.gas_tip_cap.is_zero() { *RANDOM_GAS_TIP_CAP } else { message.gas_tip_cap };
    let gas_fee_cap =
        if message.gas_fee_cap.is_zero() { *RANDOM_GAS_FEE_CAP } else { message.gas_fee_cap };
    // During gas estimation we don't want the gas limit variability to change the L1 cost.
    let gas = if message.gas_limit == 0 || message.is_gas_estimation { *RANDOM_GAS } else { message.gas_limit };
    FakeTx {
        encoded: fake_eip1559_bytes(
            nonce,
            gas_tip_cap,
            gas_fee_cap,
            gas,
            message.to,
            message.value,
            &message.data,
            &message.access_list,
        ),
    }
}

use crate::{
    burn::Burner,
    l1pricing::batch_poster::{BatchPostersTable, OpenPosterResult},
    storage::storage::{
        Storage, StorageBackedAddress, StorageBackedBigInt, StorageBackedBigUint,
        StorageBackedInt64, StorageBackedUint64,
    },
};

// var (
//     BatchPosterTableKey     = []byte{0}
//     BatchPosterAddress      = common.HexToAddress("0xA4B000000000000000000073657175656e636572")
//     BatchPosterPayToAddress = BatchPosterAddress
// )
const BATCH_POSTER_TABLE_KEY: &[u8] = &[0];
const BATCH_POSTER_ADDRESS: Address = address!("A4B000000000000000000073657175656e636572");
// types.L1PricerFundsPoolAddress in Go (go-ethereum/core/types/arbitrum_types.go).
pub(super) const L1_PRICER_FUNDS_POOL_ADDRESS: Address = address!("000000000000000000000000000000000000006c");

// const (
//     payRewardsToOffset uint64 = iota   // 0
//     equilibrationUnitsOffset           // 1
//     inertiaOffset                      // 2
//     perUnitRewardOffset                // 3
//     lastUpdateTimeOffset               // 4
//     fundsDueForRewardsOffset           // 5
//     unitsSinceOffset                   // 6
//     pricePerUnitOffset                 // 7
//     lastSurplusOffset                  // 8
//     perBatchGasCostOffset              // 9
//     amortizedCostCapBipsOffset         // 10
//     l1FeesAvailableOffset              // 11
//     gasFloorPerTokenOffset             // 12
// )
const PAY_REWARDS_TO_OFFSET: u64 = 0;
const EQUILIBRATION_UNITS_OFFSET: u64 = 1;
const INERTIA_OFFSET: u64 = 2;
const PER_UNIT_REWARD_OFFSET: u64 = 3;
#[allow(dead_code)]
const LAST_UPDATE_TIME_OFFSET: u64 = 4;
const FUNDS_DUE_FOR_REWARDS_OFFSET: u64 = 5;
#[allow(dead_code)]
const UNITS_SINCE_OFFSET: u64 = 6;
const PRICE_PER_UNIT_OFFSET: u64 = 7;
#[allow(dead_code)]
const LAST_SURPLUS_OFFSET: u64 = 8;
#[allow(dead_code)]
const PER_BATCH_GAS_COST_OFFSET: u64 = 9;
#[allow(dead_code)]
const AMORTIZED_COST_CAP_BIPS_OFFSET: u64 = 10;
#[allow(dead_code)]
const L1_FEES_AVAILABLE_OFFSET: u64 = 11;
#[allow(dead_code)]
const GAS_FLOOR_PER_TOKEN_OFFSET: u64 = 12;

// const (
//     InitialInertia           = 10
//     InitialPerUnitReward     = 10
// )
// var InitialEquilibrationUnitsV0 = arbmath.UintToBig(60 * params.TxDataNonZeroGasEIP2028 * 100000)
//     params.TxDataNonZeroGasEIP2028 = 16
const INITIAL_INERTIA: u64 = 10;
const INITIAL_PER_UNIT_REWARD: u64 = 10;
const INITIAL_EQUILIBRATION_UNITS_V0: u64 = 60 * 16 * 100_000;

// ArbOS version thresholds (params.ArbosVersion_* in Go).
const ARBOS_VERSION_3: u64 = 3;
const ARBOS_VERSION_7: u64 = 7;
const ARBOS_VERSION_10: u64 = 10;
const ARBOS_VERSION_50: u64 = 50;

pub struct L1PricingState<B: Burner> {
    // parameters
    pub(super) batch_poster_table: BatchPostersTable<B>,
    pay_rewards_to: StorageBackedAddress<B>,
    equilibration_units: StorageBackedBigUint<B>,
    inertia: StorageBackedUint64<B>,
    per_unit_reward: StorageBackedUint64<B>,

    // variables
    last_update_time: StorageBackedUint64<B>,
    funds_due_for_rewards: StorageBackedBigInt<B>,
    units_since_update: StorageBackedUint64<B>,
    price_per_unit: StorageBackedBigUint<B>,
    last_surplus: StorageBackedBigInt<B>,
    per_batch_gas_cost: StorageBackedInt64<B>,
    amortized_cost_cap_bips: StorageBackedUint64<B>,
    l1_fees_available: StorageBackedBigUint<B>,
    gas_floor_per_token: StorageBackedUint64<B>,

    arbos_version: u64,
}

/// Error returned by `update_for_batch_poster_spending`.
/// Mirrors Go's dual error surface: DB I/O failures vs. semantic time validation.
#[derive(Debug)]
pub enum UpdateSpendingError<E> {
    Db(E),
    InvalidTime,
}

// type L1PricingState struct {
// 	storage *storage.Storage

// 	// parameters
// 	batchPosterTable   *BatchPostersTable
// 	payRewardsTo       storage.StorageBackedAddress
// 	equilibrationUnits storage.StorageBackedBigUint
// 	inertia            storage.StorageBackedUint64
// 	perUnitReward      storage.StorageBackedUint64
// 	// variables
// 	lastUpdateTime     storage.StorageBackedUint64 // timestamp of the last update from L1 that we processed
// 	fundsDueForRewards storage.StorageBackedBigInt
// 	// funds collected since update are recorded as the balance in account L1PricerFundsPoolAddress
// 	unitsSinceUpdate     storage.StorageBackedUint64  // calldata units collected for since last update
// 	pricePerUnit         storage.StorageBackedBigUint // current price per calldata unit
// 	lastSurplus          storage.StorageBackedBigInt  // introduced in ArbOS version 2
// 	perBatchGasCost      storage.StorageBackedInt64   // introduced in ArbOS version 3
// 	amortizedCostCapBips storage.StorageBackedUint64  // in basis points; introduced in ArbOS version 3
// 	l1FeesAvailable      storage.StorageBackedBigUint
// 	gasFloorPerToken     storage.StorageBackedUint64 // introduced in arbos version 50, default 0

// 	ArbosVersion uint64
// }

// var (
// 	BatchPosterTableKey     = []byte{0}
// 	BatchPosterAddress      = common.HexToAddress("0xA4B000000000000000000073657175656e636572")
// 	BatchPosterPayToAddress = BatchPosterAddress

// 	ErrInvalidTime = errors.New("invalid timestamp")
// )

// const (
// 	payRewardsToOffset uint64 = iota
// 	equilibrationUnitsOffset
// 	inertiaOffset
// 	perUnitRewardOffset
// 	lastUpdateTimeOffset
// 	fundsDueForRewardsOffset
// 	unitsSinceOffset
// 	pricePerUnitOffset
// 	lastSurplusOffset
// 	perBatchGasCostOffset
// 	amortizedCostCapBipsOffset
// 	l1FeesAvailableOffset
// 	gasFloorPerTokenOffset
// )

// const (
// 	InitialInertia            = 10
// 	InitialPerUnitReward      = 10
// 	InitialPerBatchGasCostV6  = 100_000
// 	InitialPerBatchGasCostV12 = 210_000 // overridden as part of the upgrade
// )

// // one minute at 100000 bytes / sec
// var InitialEquilibrationUnitsV0 = arbmath.UintToBig(60 * params.TxDataNonZeroGasEIP2028 * 100000)
// var InitialEquilibrationUnitsV6 = arbmath.UintToBig(params.TxDataNonZeroGasEIP2028 * 10000000)

// func InitializeL1PricingState(sto *storage.Storage, initialRewardsRecipient common.Address, initialL1BaseFee *big.Int) error {
// 	bptStorage := sto.OpenCachedSubStorage(BatchPosterTableKey)
// 	if err := InitializeBatchPostersTable(bptStorage); err != nil {
// 		return err
// 	}
// 	bpTable := OpenBatchPostersTable(bptStorage)
// 	if _, err := bpTable.AddPoster(BatchPosterAddress, BatchPosterPayToAddress); err != nil {
// 		return err
// 	}
// 	if err := sto.SetByUint64(payRewardsToOffset, util.AddressToHash(initialRewardsRecipient)); err != nil {
// 		return err
// 	}
// 	equilibrationUnits := sto.OpenStorageBackedBigUint(equilibrationUnitsOffset)
// 	if err := equilibrationUnits.SetChecked(InitialEquilibrationUnitsV0); err != nil {
// 		return err
// 	}
// 	if err := sto.SetUint64ByUint64(inertiaOffset, InitialInertia); err != nil {
// 		return err
// 	}
// 	fundsDueForRewards := sto.OpenStorageBackedBigInt(fundsDueForRewardsOffset)
// 	if err := fundsDueForRewards.SetChecked(common.Big0); err != nil {
// 		return err
// 	}
// 	if err := sto.SetUint64ByUint64(perUnitRewardOffset, InitialPerUnitReward); err != nil {
// 		return err
// 	}
// 	pricePerUnit := sto.OpenStorageBackedBigInt(pricePerUnitOffset)
// 	if err := pricePerUnit.SetSaturatingWithWarning(initialL1BaseFee, "initial L1 base fee (storing in price per unit)"); err != nil {
// 		return err
// 	}
// 	return nil
// }
pub fn initialize_l1_pricing_state<B: Burner + Clone, CTX: ContextTr>(
    sto: &Storage<B>,
    ctx: &mut CTX,
    initial_rewards_recipient: Address,
    initial_l1_base_fee: U256,
) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
    let bpt_storage = sto.open_cached_sub_storage(BATCH_POSTER_TABLE_KEY);
    BatchPostersTable::initialize(&bpt_storage, ctx)?;
    let mut bp_table = BatchPostersTable::open(&bpt_storage);
    // BATCH_POSTER_PAY_TO_ADDRESS == BATCH_POSTER_ADDRESS in Go.
    bp_table
        .add_poster(ctx, BATCH_POSTER_ADDRESS, BATCH_POSTER_ADDRESS)
        .map_err(|e| match e {
            OpenPosterResult::Db(e) => e,
            OpenPosterResult::Semantic(e) => {
                panic!("unexpected semantic error adding initial batch poster: {e:?}")
            }
        })?;

    // Store the rewards recipient address (Go: sto.SetByUint64(payRewardsToOffset, util.AddressToHash(...))).
    let mut pay_rewards_to = sto.open_storage_backed_address(PAY_REWARDS_TO_OFFSET);
    pay_rewards_to.set(ctx, initial_rewards_recipient)?;

    let mut equilibration_units = sto.open_storage_backed_big_uint(EQUILIBRATION_UNITS_OFFSET);
    equilibration_units.set_checked(ctx, U256::from(INITIAL_EQUILIBRATION_UNITS_V0))?;

    sto.set_uint64_by_uint64(ctx, INERTIA_OFFSET, INITIAL_INERTIA)?;

    let mut funds_due_for_rewards = sto.open_storage_backed_big_int(FUNDS_DUE_FOR_REWARDS_OFFSET);
    funds_due_for_rewards.set_checked(ctx, revm::primitives::I256::ZERO)?;

    sto.set_uint64_by_uint64(ctx, PER_UNIT_REWARD_OFFSET, INITIAL_PER_UNIT_REWARD)?;

    // Go opens pricePerUnit as StorageBackedBigInt for SetSaturatingWithWarning;
    // we use StorageBackedBigUint (same slot, same bytes) since the value is non-negative.
    let mut price_per_unit = sto.open_storage_backed_big_uint(PRICE_PER_UNIT_OFFSET);
    price_per_unit.set_saturating_with_warning(
        ctx,
        initial_l1_base_fee,
        "initial L1 base fee (storing in price per unit)",
    )?;

    Ok(())
}

// func OpenL1PricingState(sto *storage.Storage, arbosVersion uint64) *L1PricingState {
// 	return &L1PricingState{
// 		storage:              sto,
// 		batchPosterTable:     OpenBatchPostersTable(sto.OpenCachedSubStorage(BatchPosterTableKey)),
// 		payRewardsTo:         sto.OpenStorageBackedAddress(payRewardsToOffset),
// 		equilibrationUnits:   sto.OpenStorageBackedBigUint(equilibrationUnitsOffset),
// 		inertia:              sto.OpenStorageBackedUint64(inertiaOffset),
// 		perUnitReward:        sto.OpenStorageBackedUint64(perUnitRewardOffset),
// 		lastUpdateTime:       sto.OpenStorageBackedUint64(lastUpdateTimeOffset),
// 		fundsDueForRewards:   sto.OpenStorageBackedBigInt(fundsDueForRewardsOffset),
// 		unitsSinceUpdate:     sto.OpenStorageBackedUint64(unitsSinceOffset),
// 		pricePerUnit:         sto.OpenStorageBackedBigUint(pricePerUnitOffset),
// 		lastSurplus:          sto.OpenStorageBackedBigInt(lastSurplusOffset),
// 		perBatchGasCost:      sto.OpenStorageBackedInt64(perBatchGasCostOffset),
// 		amortizedCostCapBips: sto.OpenStorageBackedUint64(amortizedCostCapBipsOffset),
// 		l1FeesAvailable:      sto.OpenStorageBackedBigUint(l1FeesAvailableOffset),
// 		gasFloorPerToken:     sto.OpenStorageBackedUint64(gasFloorPerTokenOffset),
// 		ArbosVersion:         arbosVersion,
// 	}
// }
pub fn open_l1_pricing_state<B: Burner + Clone>(sto: &Storage<B>, arbos_version: u64) -> L1PricingState<B> {
    L1PricingState {
        batch_poster_table:     BatchPostersTable::open(&sto.open_cached_sub_storage(BATCH_POSTER_TABLE_KEY)),
        pay_rewards_to:         sto.open_storage_backed_address(PAY_REWARDS_TO_OFFSET),
        equilibration_units:    sto.open_storage_backed_big_uint(EQUILIBRATION_UNITS_OFFSET),
        inertia:                sto.open_storage_backed_uint64(INERTIA_OFFSET),
        per_unit_reward:        sto.open_storage_backed_uint64(PER_UNIT_REWARD_OFFSET),
        last_update_time:       sto.open_storage_backed_uint64(LAST_UPDATE_TIME_OFFSET),
        funds_due_for_rewards:  sto.open_storage_backed_big_int(FUNDS_DUE_FOR_REWARDS_OFFSET),
        units_since_update:     sto.open_storage_backed_uint64(UNITS_SINCE_OFFSET),
        price_per_unit:         sto.open_storage_backed_big_uint(PRICE_PER_UNIT_OFFSET),
        last_surplus:           sto.open_storage_backed_big_int(LAST_SURPLUS_OFFSET),
        per_batch_gas_cost:     sto.open_storage_backed_int64(PER_BATCH_GAS_COST_OFFSET),
        amortized_cost_cap_bips: sto.open_storage_backed_uint64(AMORTIZED_COST_CAP_BIPS_OFFSET),
        l1_fees_available:      sto.open_storage_backed_big_uint(L1_FEES_AVAILABLE_OFFSET),
        gas_floor_per_token:    sto.open_storage_backed_uint64(GAS_FLOOR_PER_TOKEN_OFFSET),
        arbos_version,
    }
}

// func (ps *L1PricingState) BatchPosterTable() *BatchPostersTable {
// 	return ps.batchPosterTable
// }

// func (ps *L1PricingState) PayRewardsTo() (common.Address, error) {
// 	return ps.payRewardsTo.Get()
// }

// func (ps *L1PricingState) SetPayRewardsTo(addr common.Address) error {
// 	return ps.payRewardsTo.Set(addr)
// }

// func (ps *L1PricingState) EquilibrationUnits() (*big.Int, error) {
// 	return ps.equilibrationUnits.Get()
// }

// func (ps *L1PricingState) SetEquilibrationUnits(equilUnits *big.Int) error {
// 	return ps.equilibrationUnits.SetChecked(equilUnits)
// }

// func (ps *L1PricingState) Inertia() (uint64, error) {
// 	return ps.inertia.Get()
// }

// func (ps *L1PricingState) SetInertia(inertia uint64) error {
// 	return ps.inertia.Set(inertia)
// }

// func (ps *L1PricingState) PerUnitReward() (uint64, error) {
// 	return ps.perUnitReward.Get()
// }

// func (ps *L1PricingState) SetPerUnitReward(weiPerUnit uint64) error {
// 	return ps.perUnitReward.Set(weiPerUnit)
// }

// func (ps *L1PricingState) LastUpdateTime() (uint64, error) {
// 	return ps.lastUpdateTime.Get()
// }

// func (ps *L1PricingState) SetLastUpdateTime(t uint64) error {
// 	return ps.lastUpdateTime.Set(t)
// }

// func (ps *L1PricingState) FundsDueForRewards() (*big.Int, error) {
// 	return ps.fundsDueForRewards.Get()
// }

// func (ps *L1PricingState) SetFundsDueForRewards(amt *big.Int) error {
// 	return ps.fundsDueForRewards.SetSaturatingWithWarning(amt, "L1 pricer funds due for rewards")

// }

// func (ps *L1PricingState) UnitsSinceUpdate() (uint64, error) {
// 	return ps.unitsSinceUpdate.Get()
// }

// func (ps *L1PricingState) SetUnitsSinceUpdate(units uint64) error {
// 	return ps.unitsSinceUpdate.Set(units)
// }
impl<B: Burner> L1PricingState<B> {
    // func (ps *L1PricingState) BatchPosterTable() *BatchPostersTable {
    //     return ps.batchPosterTable
    // }
    pub fn batch_poster_table(&mut self) -> &mut BatchPostersTable<B> {
        &mut self.batch_poster_table
    }

    // func (ps *L1PricingState) PayRewardsTo() (common.Address, error) {
    //     return ps.payRewardsTo.Get()
    // }
    pub fn pay_rewards_to<Db: Database>(&self, db: &mut Db) -> Result<Address, Db::Error> {
        self.pay_rewards_to.get(db)
    }

    // func (ps *L1PricingState) SetPayRewardsTo(addr common.Address) error {
    //     return ps.payRewardsTo.Set(addr)
    // }
    pub fn set_pay_rewards_to<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        addr: Address,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.pay_rewards_to.set(ctx, addr)
    }

    // func (ps *L1PricingState) EquilibrationUnits() (*big.Int, error) {
    //     return ps.equilibrationUnits.Get()
    // }
    pub fn equilibration_units<Db: Database>(&self, db: &mut Db) -> Result<U256, Db::Error> {
        self.equilibration_units.get(db)
    }

    // func (ps *L1PricingState) SetEquilibrationUnits(equilUnits *big.Int) error {
    //     return ps.equilibrationUnits.SetChecked(equilUnits)
    // }
    pub fn set_equilibration_units<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        equil_units: U256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.equilibration_units.set_checked(ctx, equil_units)
    }

    // func (ps *L1PricingState) Inertia() (uint64, error) {
    //     return ps.inertia.Get()
    // }
    pub fn inertia<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.inertia.get(db)
    }

    // func (ps *L1PricingState) SetInertia(inertia uint64) error {
    //     return ps.inertia.Set(inertia)
    // }
    pub fn set_inertia<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        inertia: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.inertia.set(ctx, inertia)
    }

    // func (ps *L1PricingState) PerUnitReward() (uint64, error) {
    //     return ps.perUnitReward.Get()
    // }
    pub fn per_unit_reward<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.per_unit_reward.get(db)
    }

    // func (ps *L1PricingState) SetPerUnitReward(weiPerUnit uint64) error {
    //     return ps.perUnitReward.Set(weiPerUnit)
    // }
    pub fn set_per_unit_reward<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        wei_per_unit: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.per_unit_reward.set(ctx, wei_per_unit)
    }

    // func (ps *L1PricingState) LastUpdateTime() (uint64, error) {
    //     return ps.lastUpdateTime.Get()
    // }
    pub fn last_update_time<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.last_update_time.get(db)
    }

    // func (ps *L1PricingState) SetLastUpdateTime(t uint64) error {
    //     return ps.lastUpdateTime.Set(t)
    // }
    pub fn set_last_update_time<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        t: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.last_update_time.set(ctx, t)
    }

    // func (ps *L1PricingState) FundsDueForRewards() (*big.Int, error) {
    //     return ps.fundsDueForRewards.Get()
    // }
    pub fn funds_due_for_rewards<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        self.funds_due_for_rewards.get(db)
    }

    // func (ps *L1PricingState) SetFundsDueForRewards(amt *big.Int) error {
    //     return ps.fundsDueForRewards.SetSaturatingWithWarning(amt, "L1 pricer funds due for rewards")
    // }
    pub fn set_funds_due_for_rewards<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        amt: I256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.funds_due_for_rewards
            .set_saturating_with_warning(ctx, amt, "L1 pricer funds due for rewards")
    }

    // func (ps *L1PricingState) UnitsSinceUpdate() (uint64, error) {
    //     return ps.unitsSinceUpdate.Get()
    // }
    pub fn units_since_update<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.units_since_update.get(db)
    }

    // func (ps *L1PricingState) SetUnitsSinceUpdate(units uint64) error {
    //     return ps.unitsSinceUpdate.Set(units)
    // }
    pub fn set_units_since_update<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        units: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.units_since_update.set(ctx, units)
    }

    // func (ps *L1PricingState) GetL1PricingSurplus() (*big.Int, error) {
    //     fundsDueForRefunds, err := ps.BatchPosterTable().TotalFundsDue()
    //     fundsDueForRewards, err := ps.FundsDueForRewards()
    //     haveFunds, err := ps.L1FeesAvailable()
    //     needFunds := arbmath.BigAdd(fundsDueForRefunds, fundsDueForRewards)
    //     return arbmath.BigSub(haveFunds, needFunds), nil
    // }
    pub fn get_l1_pricing_surplus<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        let funds_due_for_refunds = self.batch_poster_table.total_funds_due(db)?;
        let funds_due_for_rewards = self.funds_due_for_rewards.get(db)?;
        let have_funds = self.l1_fees_available.get(db)?;
        let need_funds = funds_due_for_refunds.saturating_add(funds_due_for_rewards);
        // haveFunds (U256) − needFunds (I256); saturate U256 → I256 for the subtraction.
        let have_funds_signed = I256::try_from(have_funds).unwrap_or(I256::MAX);
        Ok(have_funds_signed.saturating_sub(need_funds))
    }

    // func (ps *L1PricingState) LastSurplus() (*big.Int, error) {
    //     return ps.lastSurplus.Get()
    // }
    pub fn last_surplus<Db: Database>(&self, db: &mut Db) -> Result<I256, Db::Error> {
        self.last_surplus.get(db)
    }

    // func (ps *L1PricingState) SetLastSurplus(val *big.Int, arbosVersion uint64) error {
    //     if arbosVersion < params.ArbosVersion_7 {
    //         return ps.lastSurplus.Set_preVersion7(val)
    //     }
    //     return ps.lastSurplus.SetSaturatingWithWarning(val, "L1 pricer last surplus")
    // }
    pub fn set_last_surplus<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: I256,
        arbos_version: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        if arbos_version < ARBOS_VERSION_7 {
            self.last_surplus.set_pre_version7(ctx, val)
        } else {
            self.last_surplus.set_saturating_with_warning(ctx, val, "L1 pricer last surplus")
        }
    }

    // func (ps *L1PricingState) AddToUnitsSinceUpdate(units uint64) error {
    //     oldUnits, err := ps.unitsSinceUpdate.Get()
    //     return ps.unitsSinceUpdate.Set(oldUnits + units)
    // }
    pub fn add_to_units_since_update<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        units: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let old_units = self.units_since_update.get(ctx.db_mut())?;
        self.units_since_update.set(ctx, old_units.saturating_add(units))
    }

    // func (ps *L1PricingState) PricePerUnit() (*big.Int, error) {
    //     return ps.pricePerUnit.Get()
    // }
    pub fn price_per_unit<Db: Database>(&self, db: &mut Db) -> Result<U256, Db::Error> {
        self.price_per_unit.get(db)
    }

    // func (ps *L1PricingState) SetPricePerUnit(price *big.Int) error {
    //     return ps.pricePerUnit.SetChecked(price)
    // }
    pub fn set_price_per_unit<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        price: U256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.price_per_unit.set_checked(ctx, price)
    }

    // func (ps *L1PricingState) SetParentGasFloorPerToken(floor uint64) error {
    //     if ps.ArbosVersion < params.ArbosVersion_50 {
    //         return fmt.Errorf("not supported")
    //     }
    //     return ps.gasFloorPerToken.Set(floor)
    // }
    pub fn set_parent_gas_floor_per_token<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        floor: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        assert!(
            self.arbos_version >= ARBOS_VERSION_50,
            "SetParentGasFloorPerToken requires ArbOS v{ARBOS_VERSION_50}+"
        );
        self.gas_floor_per_token.set(ctx, floor)
    }

    // func (ps *L1PricingState) ParentGasFloorPerToken() (uint64, error) {
    //     if ps.ArbosVersion < params.ArbosVersion_50 {
    //         return 0, nil
    //     }
    //     return ps.gasFloorPerToken.Get()
    // }
    pub fn parent_gas_floor_per_token<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        if self.arbos_version < ARBOS_VERSION_50 {
            return Ok(0);
        }
        self.gas_floor_per_token.get(db)
    }

    // func (ps *L1PricingState) PerBatchGasCost() (int64, error) {
    //     return ps.perBatchGasCost.Get()
    // }
    pub fn per_batch_gas_cost<Db: Database>(&self, db: &mut Db) -> Result<i64, Db::Error> {
        self.per_batch_gas_cost.get(db)
    }

    // func (ps *L1PricingState) SetPerBatchGasCost(cost int64) error {
    //     return ps.perBatchGasCost.Set(cost)
    // }
    pub fn set_per_batch_gas_cost<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        cost: i64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.per_batch_gas_cost.set(ctx, cost)
    }

    // func (ps *L1PricingState) AmortizedCostCapBips() (uint64, error) {
    //     return ps.amortizedCostCapBips.Get()
    // }
    pub fn amortized_cost_cap_bips<Db: Database>(&self, db: &mut Db) -> Result<u64, Db::Error> {
        self.amortized_cost_cap_bips.get(db)
    }

    // func (ps *L1PricingState) SetAmortizedCostCapBips(cap uint64) error {
    //     return ps.amortizedCostCapBips.Set(cap)
    // }
    pub fn set_amortized_cost_cap_bips<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        cap: u64,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.amortized_cost_cap_bips.set(ctx, cap)
    }

    // func (ps *L1PricingState) L1FeesAvailable() (*big.Int, error) {
    //     return ps.l1FeesAvailable.Get()
    // }
    pub fn l1_fees_available<Db: Database>(&self, db: &mut Db) -> Result<U256, Db::Error> {
        self.l1_fees_available.get(db)
    }

    // func (ps *L1PricingState) SetL1FeesAvailable(val *big.Int) error {
    //     return ps.l1FeesAvailable.SetChecked(val)
    // }
    pub fn set_l1_fees_available<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        val: U256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        self.l1_fees_available.set_checked(ctx, val)
    }

    // func (ps *L1PricingState) AddToL1FeesAvailable(delta *big.Int) (*big.Int, error) {
    //     old, err := ps.L1FeesAvailable()
    //     new := new(big.Int).Add(old, delta)
    //     if err := ps.SetL1FeesAvailable(new); err != nil { ... }
    //     return new, nil
    // }
    pub fn add_to_l1_fees_available<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        delta: U256,
    ) -> Result<U256, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        let old = self.l1_fees_available.get(ctx.db_mut())?;
        let new = old.saturating_add(delta);
        self.l1_fees_available.set_checked(ctx, new)?;
        Ok(new)
    }

    // func (ps *L1PricingState) TransferFromL1FeesAvailable(
    //     recipient common.Address, amount *big.Int, evm *vm.EVM,
    //     scenario util.TracingScenario, reason tracing.BalanceChangeReason,
    // ) (*big.Int, error) {
    //     if err := util.TransferBalance(&types.L1PricerFundsPoolAddress, &recipient, amount, evm, scenario, reason); err != nil { return nil, err }
    //     old, _ := ps.L1FeesAvailable()
    //     updated := new(big.Int).Sub(old, amount)
    //     if updated.Sign() < 0 { return nil, core.ErrInsufficientFunds }
    //     ps.SetL1FeesAvailable(updated)
    //     return updated, nil
    // }
    pub fn transfer_from_l1_fees_available<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        recipient: Address,
        amount: U256,
    ) -> Result<U256, <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        // EVM balance transfer: L1PricerFundsPoolAddress → recipient.
        // Returns Some(TransferError) on OutOfFunds / OverflowPayment, None on success.
        if let Some(err) = ctx.journal_mut().transfer(L1_PRICER_FUNDS_POOL_ADDRESS, recipient, amount)? {
            panic!("transfer_from_l1_fees_available: EVM balance transfer failed: {err:?}");
        }
        let old = self.l1_fees_available.get(ctx.db_mut())?;
        // Go returns ErrInsufficientFunds if updated < 0; the EVM transfer above
        // would already have failed, so this is a consistency check.
        assert!(amount <= old, "transfer_from_l1_fees_available: stored l1FeesAvailable inconsistent with EVM balance");
        let updated = old - amount;
        self.l1_fees_available.set_checked(ctx, updated)?;
        Ok(updated)
    }

    // // UpdateForBatchPosterSpending updates the pricing model based on a payment by a batch poster
    // func (ps *L1PricingState) UpdateForBatchPosterSpending(
    //     statedb vm.StateDB, evm *vm.EVM, arbosVersion uint64,
    //     updateTime, currentTime uint64, batchPoster common.Address,
    //     weiSpent *big.Int, l1Basefee *big.Int, scenario util.TracingScenario,
    // ) error
    pub fn update_for_batch_poster_spending<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        arbos_version: u64,
        update_time: u64,
        current_time: u64,
        batch_poster: Address,
        mut wei_spent: U256,
        l1_basefee: U256,
    ) -> Result<(), UpdateSpendingError<<<CTX::Journal as JournalTr>::Database as Database>::Error>>
    where
        B: Clone,
    {
        if arbos_version < ARBOS_VERSION_10 {
            return self.pre_version10_update_for_batch_poster_spending(
                ctx,
                arbos_version,
                update_time,
                current_time,
                batch_poster,
                wei_spent,
                l1_basefee,
            );
        }

        let mut poster_state = self
            .batch_poster_table
            .open_poster(ctx, batch_poster, true)
            .map_err(|e| match e {
                OpenPosterResult::Db(e) => UpdateSpendingError::Db(e),
                OpenPosterResult::Semantic(e) => {
                    panic!("update_for_batch_poster_spending: unexpected semantic error: {e:?}")
                }
            })?;

        let mut funds_due_for_rewards =
            self.funds_due_for_rewards(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        let mut l1_fees_available =
            self.l1_fees_available(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;

        // Compute allocation fraction: updateTimeDelta / timeDelta of units/funds go to this update.
        let mut last_update_time =
            self.last_update_time(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        if last_update_time == 0 && update_time > 0 {
            last_update_time = update_time - 1; // first update — no prior timestamp
        }
        if update_time > current_time || update_time < last_update_time {
            return Err(UpdateSpendingError::InvalidTime);
        }
        let allocation_numerator = update_time - last_update_time;
        let allocation_denominator = current_time - last_update_time;
        let (allocation_numerator, allocation_denominator) = if allocation_denominator == 0 {
            (1u64, 1u64)
        } else {
            (allocation_numerator, allocation_denominator)
        };

        // Allocate units to this update.
        let mut units_since_update =
            self.units_since_update(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        let units_allocated =
            units_since_update.saturating_mul(allocation_numerator) / allocation_denominator;
        units_since_update -= units_allocated;
        self.set_units_since_update(ctx, units_since_update).map_err(UpdateSpendingError::Db)?;

        // Impose amortized cost cap (arbosVersion >= 3).
        if arbos_version >= ARBOS_VERSION_3 {
            let amortized_cost_cap_bips =
                self.amortized_cost_cap_bips(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            if amortized_cost_cap_bips != 0 {
                // BigMulByBips(BigMulByUint(l1Basefee, unitsAllocated), bips) = l1Basefee * units * bips / 10000
                let wei_spent_cap = l1_basefee
                    .saturating_mul(U256::from(units_allocated))
                    .saturating_mul(U256::from(amortized_cost_cap_bips))
                    / U256::from(10_000u64);
                if wei_spent_cap < wei_spent {
                    wei_spent = wei_spent_cap;
                }
            }
        }

        // Update funds due to batch poster.
        let due_to_poster =
            poster_state.funds_due(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        let wei_spent_signed = I256::try_from(wei_spent).unwrap_or(I256::MAX);
        poster_state
            .set_funds_due(ctx, due_to_poster.saturating_add(wei_spent_signed))
            .map_err(UpdateSpendingError::Db)?;

        // Accrue rewards: fundsDueForRewards += unitsAllocated * perUnitReward.
        let per_unit_reward =
            self.per_unit_reward(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        // u64 always fits in I256, so the try_from unwrap is safe.
        let units_i256 = I256::try_from(U256::from(units_allocated)).expect("u64 fits in I256");
        let per_unit_i256 = I256::try_from(U256::from(per_unit_reward)).expect("u64 fits in I256");
        funds_due_for_rewards =
            funds_due_for_rewards.saturating_add(units_i256.saturating_mul(per_unit_i256));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)
            .map_err(UpdateSpendingError::Db)?;

        // Pay rewards, as much as possible.
        let mut payment_for_rewards =
            U256::from(per_unit_reward).saturating_mul(U256::from(units_allocated));
        if l1_fees_available < payment_for_rewards {
            payment_for_rewards = l1_fees_available;
        }
        funds_due_for_rewards = funds_due_for_rewards
            .saturating_sub(I256::try_from(payment_for_rewards).unwrap_or(I256::MAX));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)
            .map_err(UpdateSpendingError::Db)?;
        let pay_rewards_to =
            self.pay_rewards_to(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        l1_fees_available = self
            .transfer_from_l1_fees_available(ctx, pay_rewards_to, payment_for_rewards)
            .map_err(UpdateSpendingError::Db)?;

        // Settle payments owed to the batch poster, as much as possible.
        let balance_due_to_poster =
            poster_state.funds_due(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        if balance_due_to_poster.is_positive() {
            let balance_due_u256 =
                U256::try_from(balance_due_to_poster).unwrap_or(U256::MAX);
            let balance_to_transfer = l1_fees_available.min(balance_due_u256);
            if !balance_to_transfer.is_zero() {
                let addr_to_pay =
                    poster_state.pay_to(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
                l1_fees_available = self
                    .transfer_from_l1_fees_available(ctx, addr_to_pay, balance_to_transfer)
                    .map_err(UpdateSpendingError::Db)?;
                let transferred_signed =
                    I256::try_from(balance_to_transfer).unwrap_or(I256::MAX);
                poster_state
                    .set_funds_due(
                        ctx,
                        balance_due_to_poster.saturating_sub(transferred_signed),
                    )
                    .map_err(UpdateSpendingError::Db)?;
            }
        }

        self.set_last_update_time(ctx, update_time).map_err(UpdateSpendingError::Db)?;

        // Adjust the price per unit.
        if units_allocated > 0 {
            let total_funds_due = self
                .batch_poster_table
                .total_funds_due(ctx.db_mut())
                .map_err(UpdateSpendingError::Db)?;
            funds_due_for_rewards =
                self.funds_due_for_rewards(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;

            // surplus = l1FeesAvailable − (totalFundsDue + fundsDueForRewards)
            let l1_fees_signed = I256::try_from(l1_fees_available).unwrap_or(I256::MAX);
            let surplus =
                l1_fees_signed.saturating_sub(total_funds_due.saturating_add(funds_due_for_rewards));

            let inertia =
                self.inertia(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            let equil_units =
                self.equilibration_units(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            let price =
                self.price_per_unit(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            let old_surplus =
                self.last_surplus(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;

            // inertiaUnits = equilUnits / inertia
            let inertia_units =
                if inertia == 0 { equil_units } else { equil_units / U256::from(inertia) };
            // allocPlusInert = inertiaUnits + unitsAllocated
            let alloc_plus_inert = inertia_units + U256::from(units_allocated);

            // desiredDerivative = −surplus / equilUnits
            let equil_signed = I256::try_from(equil_units).unwrap_or(I256::MAX);
            let desired_derivative = if equil_signed.is_zero() {
                I256::ZERO
            } else {
                surplus.saturating_neg().wrapping_div(equil_signed)
            };

            // actualDerivative = (surplus − oldSurplus) / unitsAllocated
            let actual_derivative = surplus
                .saturating_sub(old_surplus)
                .wrapping_div(I256::try_from(units_allocated).unwrap_or(I256::MAX));

            // priceChange = (desiredDerivative − actualDerivative) * unitsAllocated / allocPlusInert
            let alloc_plus_inert_signed = I256::try_from(alloc_plus_inert).unwrap_or(I256::MAX);
            let price_change = if alloc_plus_inert_signed.is_zero() {
                I256::ZERO
            } else {
                desired_derivative
                    .saturating_sub(actual_derivative)
                    .saturating_mul(I256::try_from(units_allocated).unwrap_or(I256::MAX))
                    .wrapping_div(alloc_plus_inert_signed)
            };

            self.set_last_surplus(ctx, surplus, arbos_version)
                .map_err(UpdateSpendingError::Db)?;

            // newPrice = max(0, price + priceChange)
            let new_price = if price_change >= I256::ZERO {
                price.saturating_add(U256::try_from(price_change).unwrap_or(U256::MAX))
            } else {
                let abs_change =
                    U256::try_from(price_change.wrapping_neg()).unwrap_or(U256::MAX);
                price.saturating_sub(abs_change) // saturates to 0 if price < abs_change
            };
            self.set_price_per_unit(ctx, new_price).map_err(UpdateSpendingError::Db)?;
        }

        Ok(())
    }

    // func (ps *L1PricingState) getPosterUnitsWithoutCache(tx *types.Transaction, posterAddr common.Address, brotliCompressionLevel uint64) uint64 {
    //     if posterAddr != BatchPosterAddress { return 0 }
    //     txBytes, merr := tx.MarshalBinary()
    //     txType := tx.Type()
    //     if !util.TxTypeHasPosterCosts(txType) || merr != nil { return 0 }
    //     l1Bytes, err := byteCountAfterBrotliLevel(txBytes, brotliCompressionLevel)
    //     if err != nil { panic(...) }
    //     return arbmath.SaturatingUMul(params.TxDataNonZeroGasEIP2028, l1Bytes)
    // }
    pub fn get_poster_units_without_cache<T: PosterTransaction>(
        &self,
        tx: &T,
        poster_addr: Address,
        brotli_compression_level: u64,
    ) -> u64 {
        if poster_addr != BATCH_POSTER_ADDRESS {
            return 0;
        }
        let tx_type = tx.tx_type();
        if !tx_type_has_poster_costs(tx_type) {
            return 0;
        }
        let tx_bytes = tx.encode();
        let l1_bytes = byte_count_after_brotli_level(&tx_bytes, brotli_compression_level);
        TX_DATA_NON_ZERO_GAS_EIP2028.saturating_mul(l1_bytes)
    }

    // // GetPosterInfo returns the poster cost and the calldata units for a transaction
    // func (ps *L1PricingState) GetPosterInfo(tx *types.Transaction, poster common.Address, brotliCompressionLevel uint64) (*big.Int, uint64) {
    //     if poster != BatchPosterAddress { return common.Big0, 0 }
    //     var units uint64
    //     if cachedUnits := tx.GetCachedCalldataUnits(brotliCompressionLevel); cachedUnits != nil {
    //         units = *cachedUnits
    //     } else {
    //         units = ps.getPosterUnitsWithoutCache(tx, poster, brotliCompressionLevel)
    //         tx.SetCachedCalldataUnits(brotliCompressionLevel, units)
    //     }
    //     pricePerUnit, _ := ps.PricePerUnit()
    //     return arbmath.BigMulByUint(pricePerUnit, units), units
    // }
    pub fn get_poster_info<T: PosterTransaction, Db: Database>(
        &self,
        db: &mut Db,
        tx: &mut T,
        poster: Address,
        brotli_compression_level: u64,
    ) -> Result<(U256, u64), Db::Error> {
        if poster != BATCH_POSTER_ADDRESS {
            return Ok((U256::ZERO, 0));
        }
        let units = match tx.get_cached_calldata_units(brotli_compression_level) {
            Some(cached) => cached,
            None => {
                let computed = self.get_poster_units_without_cache(tx, poster, brotli_compression_level);
                tx.set_cached_calldata_units(brotli_compression_level, computed);
                computed
            }
        };
        let price_per_unit = self.price_per_unit(db)?;
        Ok((price_per_unit.saturating_mul(U256::from(units)), units))
    }

    // func (ps *L1PricingState) PosterDataCost(message *core.Message, poster common.Address, brotliCompressionLevel uint64) (*big.Int, uint64)
    pub fn poster_data_cost<Db: Database>(
        &self,
        db: &mut Db,
        message: &mut Message,
        poster: Address,
        brotli_compression_level: u64,
    ) -> Result<(U256, u64), Db::Error> {
        // Real-transaction path: delegate to GetPosterInfo (which reads/writes the units cache).
        if let Some(ref mut tx) = message.tx {
            return self.get_poster_info(db, tx, poster, brotli_compression_level);
        }

        // Gas-estimation path: no real tx available; build a fake one and pad the cost.
        let fake = make_fake_tx_for_message(message);
        let units = self.get_poster_units_without_cache(&fake, poster, brotli_compression_level);

        // arbmath.UintMulByBips(units + estimationPaddingUnits, OneInBips + estimationPaddingBasisPoints)
        const ONE_IN_BIPS: u64 = 10_000;
        let units = units
            .saturating_add(ESTIMATION_PADDING_UNITS)
            .saturating_mul(ONE_IN_BIPS + ESTIMATION_PADDING_BASIS_POINTS)
            / ONE_IN_BIPS;

        let price_per_unit = self.price_per_unit(db)?;
        Ok((price_per_unit.saturating_mul(U256::from(units)), units))
    }
}

// func (ps *L1PricingState) GetL1PricingSurplus() (*big.Int, error) {
// 	fundsDueForRefunds, err := ps.BatchPosterTable().TotalFundsDue()
// 	if err != nil {
// 		return nil, err
// 	}
// 	fundsDueForRewards, err := ps.FundsDueForRewards()
// 	if err != nil {
// 		return nil, err
// 	}
// 	haveFunds, err := ps.L1FeesAvailable()
// 	if err != nil {
// 		return nil, err
// 	}
// 	needFunds := arbmath.BigAdd(fundsDueForRefunds, fundsDueForRewards)
// 	return arbmath.BigSub(haveFunds, needFunds), nil
// }

// func (ps *L1PricingState) LastSurplus() (*big.Int, error) {
// 	return ps.lastSurplus.Get()
// }

// func (ps *L1PricingState) SetLastSurplus(val *big.Int, arbosVersion uint64) error {
// 	if arbosVersion < params.ArbosVersion_7 {
// 		return ps.lastSurplus.Set_preVersion7(val)
// 	}
// 	return ps.lastSurplus.SetSaturatingWithWarning(val, "L1 pricer last surplus")
// }

// func (ps *L1PricingState) AddToUnitsSinceUpdate(units uint64) error {
// 	oldUnits, err := ps.unitsSinceUpdate.Get()
// 	if err != nil {
// 		return err
// 	}
// 	return ps.unitsSinceUpdate.Set(oldUnits + units)
// }

// func (ps *L1PricingState) PricePerUnit() (*big.Int, error) {
// 	return ps.pricePerUnit.Get()
// }

// func (ps *L1PricingState) SetPricePerUnit(price *big.Int) error {
// 	return ps.pricePerUnit.SetChecked(price)
// }

// func (ps *L1PricingState) SetParentGasFloorPerToken(floor uint64) error {
// 	if ps.ArbosVersion < params.ArbosVersion_50 {
// 		return fmt.Errorf("not supported")
// 	}
// 	return ps.gasFloorPerToken.Set(floor)
// }

// func (ps *L1PricingState) ParentGasFloorPerToken() (uint64, error) {
// 	if ps.ArbosVersion < params.ArbosVersion_50 {
// 		return 0, nil
// 	}
// 	return ps.gasFloorPerToken.Get()
// }

// func (ps *L1PricingState) PerBatchGasCost() (int64, error) {
// 	return ps.perBatchGasCost.Get()
// }

// func (ps *L1PricingState) SetPerBatchGasCost(cost int64) error {
// 	return ps.perBatchGasCost.Set(cost)
// }

// func (ps *L1PricingState) AmortizedCostCapBips() (uint64, error) {
// 	return ps.amortizedCostCapBips.Get()
// }

// func (ps *L1PricingState) SetAmortizedCostCapBips(cap uint64) error {
// 	return ps.amortizedCostCapBips.Set(cap)
// }

// func (ps *L1PricingState) L1FeesAvailable() (*big.Int, error) {
// 	return ps.l1FeesAvailable.Get()
// }

// func (ps *L1PricingState) SetL1FeesAvailable(val *big.Int) error {
// 	return ps.l1FeesAvailable.SetChecked(val)
// }

// func (ps *L1PricingState) AddToL1FeesAvailable(delta *big.Int) (*big.Int, error) {
// 	old, err := ps.L1FeesAvailable()
// 	if err != nil {
// 		return nil, err
// 	}
// 	new := new(big.Int).Add(old, delta)
// 	if err := ps.SetL1FeesAvailable(new); err != nil {
// 		return nil, err
// 	}
// 	return new, nil
// }

// func (ps *L1PricingState) TransferFromL1FeesAvailable(
// 	recipient common.Address,
// 	amount *big.Int,
// 	evm *vm.EVM,
// 	scenario util.TracingScenario,
// 	reason tracing.BalanceChangeReason,
// ) (*big.Int, error) {
// 	if err := util.TransferBalance(&types.L1PricerFundsPoolAddress, &recipient, amount, evm, scenario, reason); err != nil {
// 		return nil, err
// 	}
// 	old, err := ps.L1FeesAvailable()
// 	if err != nil {
// 		return nil, err
// 	}
// 	updated := new(big.Int).Sub(old, amount)
// 	if updated.Sign() < 0 {
// 		return nil, core.ErrInsufficientFunds
// 	}
// 	if err := ps.SetL1FeesAvailable(updated); err != nil {
// 		return nil, err
// 	}
// 	return updated, nil
// }

// // UpdateForBatchPosterSpending updates the pricing model based on a payment by a batch poster
// func (ps *L1PricingState) UpdateForBatchPosterSpending(
// 	statedb vm.StateDB,
// 	evm *vm.EVM,
// 	arbosVersion uint64,
// 	updateTime, currentTime uint64,
// 	batchPoster common.Address,
// 	weiSpent *big.Int,
// 	l1Basefee *big.Int,
// 	scenario util.TracingScenario,
// ) error {
// 	if arbosVersion < params.ArbosVersion_10 {
// 		return ps._preversion10_UpdateForBatchPosterSpending(statedb, evm, arbosVersion, updateTime, currentTime, batchPoster, weiSpent, l1Basefee, scenario)
// 	}

// 	batchPosterTable := ps.BatchPosterTable()
// 	posterState, err := batchPosterTable.OpenPoster(batchPoster, true)
// 	if err != nil {
// 		return err
// 	}

// 	fundsDueForRewards, err := ps.FundsDueForRewards()
// 	if err != nil {
// 		return err
// 	}

// 	l1FeesAvailable, err := ps.L1FeesAvailable()
// 	if err != nil {
// 		return err
// 	}

// 	// compute allocation fraction -- will allocate updateTimeDelta/timeDelta fraction of units and funds to this update
// 	lastUpdateTime, err := ps.LastUpdateTime()
// 	if err != nil {
// 		return err
// 	}
// 	if lastUpdateTime == 0 && updateTime > 0 { // it's the first update, so there isn't a last update time
// 		lastUpdateTime = updateTime - 1
// 	}
// 	if updateTime > currentTime || updateTime < lastUpdateTime {
// 		return ErrInvalidTime
// 	}
// 	allocationNumerator := updateTime - lastUpdateTime
// 	allocationDenominator := currentTime - lastUpdateTime
// 	if allocationDenominator == 0 {
// 		allocationNumerator = 1
// 		allocationDenominator = 1
// 	}

// 	// allocate units to this update
// 	unitsSinceUpdate, err := ps.UnitsSinceUpdate()
// 	if err != nil {
// 		return err
// 	}
// 	unitsAllocated := arbmath.SaturatingUMul(unitsSinceUpdate, allocationNumerator) / allocationDenominator
// 	unitsSinceUpdate -= unitsAllocated
// 	if err := ps.SetUnitsSinceUpdate(unitsSinceUpdate); err != nil {
// 		return err
// 	}

// 	// impose cap on amortized cost, if there is one
// 	if arbosVersion >= params.ArbosVersion_3 {
// 		amortizedCostCapBips, err := ps.AmortizedCostCapBips()
// 		if err != nil {
// 			return err
// 		}
// 		if amortizedCostCapBips != 0 {
// 			weiSpentCap := arbmath.BigMulByBips(
// 				arbmath.BigMulByUint(l1Basefee, unitsAllocated),
// 				arbmath.SaturatingCastToBips(amortizedCostCapBips),
// 			)
// 			if arbmath.BigLessThan(weiSpentCap, weiSpent) {
// 				// apply the cap on assignment of amortized cost;
// 				// the difference will be a loss for the batch poster
// 				weiSpent = weiSpentCap
// 			}
// 		}
// 	}

// 	dueToPoster, err := posterState.FundsDue()
// 	if err != nil {
// 		return err
// 	}
// 	err = posterState.SetFundsDue(arbmath.BigAdd(dueToPoster, weiSpent))
// 	if err != nil {
// 		return err
// 	}
// 	perUnitReward, err := ps.PerUnitReward()
// 	if err != nil {
// 		return err
// 	}
// 	fundsDueForRewards = arbmath.BigAdd(fundsDueForRewards, arbmath.BigMulByUint(arbmath.UintToBig(unitsAllocated), perUnitReward))
// 	if err := ps.SetFundsDueForRewards(fundsDueForRewards); err != nil {
// 		return err
// 	}

// 	// pay rewards, as much as possible
// 	paymentForRewards := arbmath.BigMulByUint(arbmath.UintToBig(perUnitReward), unitsAllocated)
// 	if arbmath.BigLessThan(l1FeesAvailable, paymentForRewards) {
// 		paymentForRewards = l1FeesAvailable
// 	}
// 	fundsDueForRewards = arbmath.BigSub(fundsDueForRewards, paymentForRewards)
// 	if err := ps.SetFundsDueForRewards(fundsDueForRewards); err != nil {
// 		return err
// 	}
// 	payRewardsTo, err := ps.PayRewardsTo()
// 	if err != nil {
// 		return err
// 	}
// 	l1FeesAvailable, err = ps.TransferFromL1FeesAvailable(
// 		payRewardsTo, paymentForRewards, evm, scenario, tracing.BalanceChangeTransferBatchposterReward,
// 	)
// 	if err != nil {
// 		return err
// 	}

// 	// settle up payments owed to the batch poster, as much as possible
// 	balanceDueToPoster, err := posterState.FundsDue()
// 	if err != nil {
// 		return err
// 	}
// 	balanceToTransfer := balanceDueToPoster
// 	if arbmath.BigLessThan(l1FeesAvailable, balanceToTransfer) {
// 		balanceToTransfer = l1FeesAvailable
// 	}
// 	if balanceToTransfer.Sign() > 0 {
// 		addrToPay, err := posterState.PayTo()
// 		if err != nil {
// 			return err
// 		}
// 		l1FeesAvailable, err = ps.TransferFromL1FeesAvailable(
// 			addrToPay, balanceToTransfer, evm, scenario, tracing.BalanceChangeTransferBatchposterRefund,
// 		)
// 		if err != nil {
// 			return err
// 		}
// 		balanceDueToPoster = arbmath.BigSub(balanceDueToPoster, balanceToTransfer)
// 		err = posterState.SetFundsDue(balanceDueToPoster)
// 		if err != nil {
// 			return err
// 		}
// 	}

// 	// update time
// 	if err := ps.SetLastUpdateTime(updateTime); err != nil {
// 		return err
// 	}

// 	// adjust the price
// 	if unitsAllocated > 0 {
// 		totalFundsDue, err := batchPosterTable.TotalFundsDue()
// 		if err != nil {
// 			return err
// 		}
// 		fundsDueForRewards, err = ps.FundsDueForRewards()
// 		if err != nil {
// 			return err
// 		}
// 		surplus := arbmath.BigSub(l1FeesAvailable, arbmath.BigAdd(totalFundsDue, fundsDueForRewards))

// 		inertia, err := ps.Inertia()
// 		if err != nil {
// 			return err
// 		}
// 		equilUnits, err := ps.EquilibrationUnits()
// 		if err != nil {
// 			return err
// 		}
// 		inertiaUnits := arbmath.BigDivByUint(equilUnits, inertia)
// 		price, err := ps.PricePerUnit()
// 		if err != nil {
// 			return err
// 		}

// 		allocPlusInert := arbmath.BigAddByUint(inertiaUnits, unitsAllocated)
// 		oldSurplus, err := ps.LastSurplus()
// 		if err != nil {
// 			return err
// 		}

// 		desiredDerivative := arbmath.BigDiv(new(big.Int).Neg(surplus), equilUnits)
// 		actualDerivative := arbmath.BigDivByUint(arbmath.BigSub(surplus, oldSurplus), unitsAllocated)
// 		changeDerivativeBy := arbmath.BigSub(desiredDerivative, actualDerivative)
// 		priceChange := arbmath.BigDiv(arbmath.BigMulByUint(changeDerivativeBy, unitsAllocated), allocPlusInert)

// 		if err := ps.SetLastSurplus(surplus, arbosVersion); err != nil {
// 			return err
// 		}
// 		newPrice := arbmath.BigAdd(price, priceChange)
// 		if newPrice.Sign() < 0 {
// 			newPrice = common.Big0
// 		}
// 		if err := ps.SetPricePerUnit(newPrice); err != nil {
// 			return err
// 		}
// 	}
// 	return nil
// }

// func (ps *L1PricingState) getPosterUnitsWithoutCache(tx *types.Transaction, posterAddr common.Address, brotliCompressionLevel uint64) uint64 {

// 	if posterAddr != BatchPosterAddress {
// 		return 0
// 	}
// 	txBytes, merr := tx.MarshalBinary()
// 	txType := tx.Type()
// 	if !util.TxTypeHasPosterCosts(txType) || merr != nil {
// 		return 0
// 	}

// 	l1Bytes, err := byteCountAfterBrotliLevel(txBytes, brotliCompressionLevel)
// 	if err != nil {
// 		panic(fmt.Sprintf("failed to compress tx: %v", err))
// 	}
// 	return arbmath.SaturatingUMul(params.TxDataNonZeroGasEIP2028, l1Bytes)
// }

// // GetPosterInfo returns the poster cost and the calldata units for a transaction
// func (ps *L1PricingState) GetPosterInfo(tx *types.Transaction, poster common.Address, brotliCompressionLevel uint64) (*big.Int, uint64) {
// 	if poster != BatchPosterAddress {
// 		return common.Big0, 0
// 	}
// 	var units uint64
// 	if cachedUnits := tx.GetCachedCalldataUnits(brotliCompressionLevel); cachedUnits != nil {
// 		units = *cachedUnits
// 	} else {
// 		// The cache is empty or invalid, so we need to compute the calldata units
// 		units = ps.getPosterUnitsWithoutCache(tx, poster, brotliCompressionLevel)
// 		tx.SetCachedCalldataUnits(brotliCompressionLevel, units)
// 	}

// 	// Approximate the l1 fee charged for posting this tx's calldata
// 	pricePerUnit, _ := ps.PricePerUnit()
// 	return arbmath.BigMulByUint(pricePerUnit, units), units
// }

// // We don't have the full tx in gas estimation, so we assume it might be a bit bigger in practice.
// var estimationPaddingUnits uint64 = 16 * params.TxDataNonZeroGasEIP2028

// const estimationPaddingBasisPoints = 100

// var randomNonce = binary.BigEndian.Uint64(crypto.Keccak256([]byte("Nonce"))[:8])
// var randomGasTipCap = new(big.Int).SetBytes(crypto.Keccak256([]byte("GasTipCap"))[:4])
// var randomGasFeeCap = new(big.Int).SetBytes(crypto.Keccak256([]byte("GasFeeCap"))[:4])
// var RandomGas = uint64(binary.BigEndian.Uint32(crypto.Keccak256([]byte("Gas"))[:4]))
// var randV = arbmath.BigMulByUint(chaininfo.ArbitrumOneChainConfig().ChainID, 3)
// var randR = crypto.Keccak256Hash([]byte("R")).Big()
// var randS = crypto.Keccak256Hash([]byte("S")).Big()

// // The returned tx will be invalid, likely for a number of reasons such as an invalid signature.
// // It's only used to check how large it is after brotli level 0 compression.
// func makeFakeTxForMessage(message *core.Message) *types.Transaction {
// 	nonce := message.Nonce
// 	if nonce == 0 {
// 		nonce = randomNonce
// 	}
// 	gasTipCap := message.GasTipCap
// 	if gasTipCap.Sign() == 0 {
// 		gasTipCap = randomGasTipCap
// 	}
// 	gasFeeCap := message.GasFeeCap
// 	if gasFeeCap.Sign() == 0 {
// 		gasFeeCap = randomGasFeeCap
// 	}
// 	// During gas estimation, we don't want the gas limit variability to change the L1 cost.
// 	gas := message.GasLimit
// 	if gas == 0 || message.TxRunContext.IsGasEstimation() {
// 		gas = RandomGas
// 	}
// 	return types.NewTx(&types.DynamicFeeTx{
// 		Nonce:      nonce,
// 		GasTipCap:  gasTipCap,
// 		GasFeeCap:  gasFeeCap,
// 		Gas:        gas,
// 		To:         message.To,
// 		Value:      message.Value,
// 		Data:       message.Data,
// 		AccessList: message.AccessList,
// 		V:          randV,
// 		R:          randR,
// 		S:          randS,
// 	})
// }

// func (ps *L1PricingState) PosterDataCost(message *core.Message, poster common.Address, brotliCompressionLevel uint64) (*big.Int, uint64) {
// 	tx := message.Tx
// 	if tx != nil {
// 		return ps.GetPosterInfo(tx, poster, brotliCompressionLevel)
// 	}

// 	// Otherwise, we don't have an underlying transaction, so we're likely in gas estimation.
// 	// We'll instead make a fake tx from the message info we do have, and then pad our cost a bit to be safe.
// 	tx = makeFakeTxForMessage(message)
// 	units := ps.getPosterUnitsWithoutCache(tx, poster, brotliCompressionLevel)
// 	units = arbmath.UintMulByBips(units+estimationPaddingUnits, arbmath.OneInBips+estimationPaddingBasisPoints)
// 	pricePerUnit, _ := ps.PricePerUnit()
// 	return arbmath.BigMulByUint(pricePerUnit, units), units
// }

// func byteCountAfterBrotliLevel(input []byte, level uint64) (uint64, error) {
// 	compressed, err := arbcompress.CompressLevel(input, level)
// 	if err != nil {
// 		return 0, err
// 	}
// 	return uint64(len(compressed)), nil
// }
