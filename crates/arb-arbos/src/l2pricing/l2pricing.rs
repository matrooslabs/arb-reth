use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
};
use crate::{
    burn::Burner, l2pricing::multi_gas_fees::MultiGasFees, storage::{
        storage::{Storage, StorageBackedBigUint, StorageBackedUint64},
        vector::SubStorageVector,
    }
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
    multi_gas_fees: MultiGasFees<B>,
    arbos_version: u64,
}


// const (
// 	speedLimitPerSecondOffset uint64 = iota
// 	perBlockGasLimitOffset
// 	baseFeeWeiOffset
// 	minBaseFeeWeiOffset
// 	gasBacklogOffset
// 	pricingInertiaOffset
// 	backlogToleranceOffset
// 	perTxGasLimitOffset
// )

const SPEED_LIMIT_PER_SECOND_OFFSET: u64 = 0;
const PER_BLOCK_GAS_LIMIT_OFFSET: u64 = 1;
const BASE_FEE_WEI_OFFSET: u64 = 2;
const MIN_BASE_FEE_WEI_OFFSET: u64 = 3;
const GAS_BACKLOG_OFFSET: u64 = 4;
const PRICING_INERTIA_OFFSET: u64 = 5;
const BACKLOG_TOLERANCE_OFFSET: u64 = 6;
const PER_TX_GAS_LIMIT_OFFSET: u64 = 7;

// var gasConstraintsKey []byte = []byte{0}
// var multiGasConstraintsKey []byte = []byte{1}
// var multiGasBaseFeesKey []byte = []byte{2}

const GAS_CONSTRAINTS_KEY: &[u8] = &[0];
const MULTI_GAS_CONSTRAINTS_KEY: &[u8] = &[1];
const MULTI_GAS_BASE_FEES_KEY: &[u8] = &[2];

// const GethBlockGasLimit = 1 << 50

// // Number of single-gas constraints limited because of retryable redeem gas cost calculation.
// // The limit is ignored starting from ArbOS version 60.
// const GasConstraintsMaxNum = 20

// // MaxPricingExponentBips caps the basefee growth: exp(8.5) ~= x5,000 min base fee.
// const MaxPricingExponentBips = arbmath.Bips(85_000)

// From model.go:
//   const InitialSpeedLimitPerSecondV0 = 1000000
//   const InitialPerBlockGasLimitV0 uint64 = 20 * 1000000
//   const InitialMinimumBaseFeeWei = params.GWei / 10  // = 1e9 / 10
//   const InitialBaseFeeWei = InitialMinimumBaseFeeWei
//   const InitialPricingInertia = 102
//   const InitialBacklogTolerance = 10
const INITIAL_SPEED_LIMIT_PER_SECOND_V0: u64 = 1_000_000;
const INITIAL_PER_BLOCK_GAS_LIMIT_V0: u64 = 20 * 1_000_000;
const INITIAL_MINIMUM_BASE_FEE_WEI: u64 = 100_000_000; // params.GWei / 10
const INITIAL_BASE_FEE_WEI: u64 = INITIAL_MINIMUM_BASE_FEE_WEI;
const INITIAL_PRICING_INERTIA: u64 = 102;
const INITIAL_BACKLOG_TOLERANCE: u64 = 10;

impl<B: Burner> L2PricingState<B> {
    // func OpenL2PricingState(sto *storage.Storage, arbosVersion uint64) *L2PricingState
    pub fn open(sto: Storage<B>, arbos_version: u64) -> Self
    where
        B: Clone,
    {
        let speed_limit_per_second = sto.open_storage_backed_uint64(SPEED_LIMIT_PER_SECOND_OFFSET);
        let per_block_gas_limit = sto.open_storage_backed_uint64(PER_BLOCK_GAS_LIMIT_OFFSET);
        let base_fee_wei = sto.open_storage_backed_big_uint(BASE_FEE_WEI_OFFSET);
        let min_base_fee_wei = sto.open_storage_backed_big_uint(MIN_BASE_FEE_WEI_OFFSET);
        let gas_backlog = sto.open_storage_backed_uint64(GAS_BACKLOG_OFFSET);
        let pricing_inertia = sto.open_storage_backed_uint64(PRICING_INERTIA_OFFSET);
        let backlog_tolerance = sto.open_storage_backed_uint64(BACKLOG_TOLERANCE_OFFSET);
        let per_tx_gas_limit = sto.open_storage_backed_uint64(PER_TX_GAS_LIMIT_OFFSET);
        let gas_constraints =
            SubStorageVector::open(&sto.open_sub_storage(GAS_CONSTRAINTS_KEY));
        let multi_gas_constraints =
            SubStorageVector::open(&sto.open_sub_storage(MULTI_GAS_CONSTRAINTS_KEY));
        let multi_gas_fees = MultiGasFees::open(&sto.open_sub_storage(MULTI_GAS_BASE_FEES_KEY));
        L2PricingState {
            storage: sto,
            speed_limit_per_second,
            per_block_gas_limit,
            base_fee_wei,
            min_base_fee_wei,
            gas_backlog,
            pricing_inertia,
            backlog_tolerance,
            per_tx_gas_limit,
            gas_constraints,
            multi_gas_constraints,
            multi_gas_fees,
            arbos_version,
        }
    }

    // func InitializeL2PricingState(sto *storage.Storage) error
    pub fn initialize<CTX: ContextTr>(
        sto: &Storage<B>,
        ctx: &mut CTX,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error> {
        sto.set_uint64_by_uint64(ctx, SPEED_LIMIT_PER_SECOND_OFFSET, INITIAL_SPEED_LIMIT_PER_SECOND_V0)?;
        sto.set_uint64_by_uint64(ctx, PER_BLOCK_GAS_LIMIT_OFFSET, INITIAL_PER_BLOCK_GAS_LIMIT_V0)?;
        sto.set_uint64_by_uint64(ctx, BASE_FEE_WEI_OFFSET, INITIAL_BASE_FEE_WEI)?;
        sto.set_uint64_by_uint64(ctx, GAS_BACKLOG_OFFSET, 0)?;
        sto.set_uint64_by_uint64(ctx, PRICING_INERTIA_OFFSET, INITIAL_PRICING_INERTIA)?;
        sto.set_uint64_by_uint64(ctx, BACKLOG_TOLERANCE_OFFSET, INITIAL_BACKLOG_TOLERANCE)?;
        sto.set_uint64_by_uint64(ctx, MIN_BASE_FEE_WEI_OFFSET, INITIAL_MINIMUM_BASE_FEE_WEI)
    }
}

// func OpenL2PricingState(sto *storage.Storage, arbosVersion uint64) *L2PricingState {
// 	return &L2PricingState{
// 		storage:             sto,
// 		speedLimitPerSecond: sto.OpenStorageBackedUint64(speedLimitPerSecondOffset),
// 		perBlockGasLimit:    sto.OpenStorageBackedUint64(perBlockGasLimitOffset),
// 		baseFeeWei:          sto.OpenStorageBackedBigUint(baseFeeWeiOffset),
// 		minBaseFeeWei:       sto.OpenStorageBackedBigUint(minBaseFeeWeiOffset),
// 		gasBacklog:          sto.OpenStorageBackedUint64(gasBacklogOffset),
// 		pricingInertia:      sto.OpenStorageBackedUint64(pricingInertiaOffset),
// 		backlogTolerance:    sto.OpenStorageBackedUint64(backlogToleranceOffset),
// 		perTxGasLimit:       sto.OpenStorageBackedUint64(perTxGasLimitOffset),
// 		gasConstraints:      storage.OpenSubStorageVector(sto.OpenSubStorage(gasConstraintsKey)),
// 		multiGasConstraints: storage.OpenSubStorageVector(sto.OpenSubStorage(multiGasConstraintsKey)),
// 		multiGasFees:        OpenMultiGasFees(sto.OpenSubStorage(multiGasBaseFeesKey)),
// 		ArbosVersion:        arbosVersion,
// 	}
// }

// func (ps *L2PricingState) BaseFeeWei() (*big.Int, error) {
// 	return ps.baseFeeWei.Get()
// }

// func (ps *L2PricingState) SetBaseFeeWei(val *big.Int) error {
// 	return ps.baseFeeWei.SetSaturatingWithWarning(val, "L2 base fee")
// }

// func (ps *L2PricingState) MinBaseFeeWei() (*big.Int, error) {
// 	return ps.minBaseFeeWei.Get()
// }

// func (ps *L2PricingState) SetMinBaseFeeWei(val *big.Int) error {
// 	// This modifies the "minimum basefee" parameter, but doesn't modify the current basefee.
// 	// If this increases the minimum basefee, then the basefee might be below the minimum for a little while.
// 	// If so, the basefee will increase by up to a factor of two per block, until it reaches the minimum.
// 	return ps.minBaseFeeWei.SetChecked(val)
// }

// func (ps *L2PricingState) SpeedLimitPerSecond() (uint64, error) {
// 	return ps.speedLimitPerSecond.Get()
// }

// func (ps *L2PricingState) SetSpeedLimitPerSecond(limit uint64) error {
// 	return ps.speedLimitPerSecond.Set(limit)
// }

// func (ps *L2PricingState) PerBlockGasLimit() (uint64, error) {
// 	return ps.perBlockGasLimit.Get()
// }

// func (ps *L2PricingState) SetMaxPerBlockGasLimit(limit uint64) error {
// 	return ps.perBlockGasLimit.Set(limit)
// }

// func (ps *L2PricingState) PerTxGasLimit() (uint64, error) {
// 	return ps.perTxGasLimit.Get()
// }

// func (ps *L2PricingState) SetMaxPerTxGasLimit(limit uint64) error {
// 	return ps.perTxGasLimit.Set(limit)
// }

// func (ps *L2PricingState) GasBacklog() (uint64, error) {
// 	return ps.gasBacklog.Get()
// }

// func (ps *L2PricingState) SetGasBacklog(backlog uint64) error {
// 	return ps.gasBacklog.Set(backlog)
// }

// func (ps *L2PricingState) PricingInertia() (uint64, error) {
// 	return ps.pricingInertia.Get()
// }

// func (ps *L2PricingState) SetPricingInertia(val uint64) error {
// 	return ps.pricingInertia.Set(val)
// }

// func (ps *L2PricingState) BacklogTolerance() (uint64, error) {
// 	return ps.backlogTolerance.Get()
// }

// func (ps *L2PricingState) SetBacklogTolerance(val uint64) error {
// 	return ps.backlogTolerance.Set(val)
// }

// func (ps *L2PricingState) Restrict(err error) {
// 	ps.storage.Burner().Restrict(err)
// }

// func (ps *L2PricingState) setGasConstraintsFromLegacy() error {
// 	if err := ps.ClearGasConstraints(); err != nil {
// 		return err
// 	}
// 	target, err := ps.SpeedLimitPerSecond()
// 	if err != nil {
// 		return err
// 	}
// 	adjustmentWindow, err := ps.PricingInertia()
// 	if err != nil {
// 		return err
// 	}
// 	oldBacklog, err := ps.GasBacklog()
// 	if err != nil {
// 		return err
// 	}
// 	backlogTolerance, err := ps.BacklogTolerance()
// 	if err != nil {
// 		return err
// 	}
// 	backlog := arbmath.SaturatingUSub(oldBacklog, arbmath.SaturatingUMul(backlogTolerance, target))
// 	return ps.AddGasConstraint(target, adjustmentWindow, backlog)
// }

// func (ps *L2PricingState) setMultiGasConstraintsFromSingleGasConstraints() error {
// 	if err := ps.ClearMultiGasConstraints(); err != nil {
// 		return err
// 	}

// 	length, err := ps.GasConstraintsLength()
// 	if err != nil {
// 		return err
// 	}

// 	for i := range length {
// 		c := ps.OpenGasConstraintAt(i)

// 		target, err := c.Target()
// 		if err != nil {
// 			return fmt.Errorf("failed to read target from constraint %d: %w", i, err)
// 		}
// 		window, err := c.AdjustmentWindow()
// 		if err != nil {
// 			return fmt.Errorf("failed to read adjustment window from constraint %d: %w", i, err)
// 		}
// 		backlog, err := c.Backlog()
// 		if err != nil {
// 			return fmt.Errorf("failed to read backlog from constraint %d: %w", i, err)
// 		}

// 		// Transfer to multi-gas constraint with equal weights
// 		weights := map[uint8]uint64{
// 			uint8(multigas.ResourceKindComputation):     1,
// 			uint8(multigas.ResourceKindHistoryGrowth):   1,
// 			uint8(multigas.ResourceKindStorageAccess):   1,
// 			uint8(multigas.ResourceKindStorageGrowth):   1,
// 			uint8(multigas.ResourceKindL2Calldata):      1,
// 			uint8(multigas.ResourceKindWasmComputation): 1,
// 		}

// 		var adjustmentWindow uint32
// 		if window > math.MaxUint32 {
// 			adjustmentWindow = math.MaxUint32
// 		} else {
// 			adjustmentWindow = uint32(window)
// 		}

// 		if err := ps.AddMultiGasConstraint(
// 			target,
// 			adjustmentWindow,
// 			backlog,
// 			weights,
// 		); err != nil {
// 			return fmt.Errorf("failed to add multi-gas constraint %d: %w", i, err)
// 		}
// 	}
// 	return nil
// }

// func (ps *L2PricingState) AddGasConstraint(target uint64, adjustmentWindow uint64, backlog uint64) error {
// 	subStorage, err := ps.gasConstraints.Push()
// 	if err != nil {
// 		return fmt.Errorf("failed to push constraint: %w", err)
// 	}
// 	constraint := OpenGasConstraint(subStorage)
// 	if err := constraint.SetTarget(target); err != nil {
// 		return fmt.Errorf("failed to set target: %w", err)
// 	}
// 	if err := constraint.SetAdjustmentWindow(adjustmentWindow); err != nil {
// 		return fmt.Errorf("failed to set adjustment window: %w", err)
// 	}
// 	if err := constraint.SetBacklog(backlog); err != nil {
// 		return fmt.Errorf("failed to set backlog: %w", err)
// 	}
// 	return nil
// }

// func (ps *L2PricingState) GasConstraintsLength() (uint64, error) {
// 	return ps.gasConstraints.Length()
// }

// func (ps *L2PricingState) OpenGasConstraintAt(i uint64) *GasConstraint {
// 	return OpenGasConstraint(ps.gasConstraints.At(i))
// }

// func (ps *L2PricingState) ClearGasConstraints() error {
// 	length, err := ps.GasConstraintsLength()
// 	if err != nil {
// 		return err
// 	}
// 	for range length {
// 		subStorage, err := ps.gasConstraints.Pop()
// 		if err != nil {
// 			return err
// 		}
// 		constraint := OpenGasConstraint(subStorage)
// 		if err := constraint.Clear(); err != nil {
// 			return err
// 		}
// 	}
// 	return nil
// }

// func (ps *L2PricingState) MultiGasConstraintsLength() (uint64, error) {
// 	return ps.multiGasConstraints.Length()
// }

// func (ps *L2PricingState) OpenMultiGasConstraintAt(i uint64) *MultiGasConstraint {
// 	return OpenMultiGasConstraint(ps.multiGasConstraints.At(i))
// }

// func (ps *L2PricingState) AddMultiGasConstraint(
// 	target uint64,
// 	adjustmentWindow uint32,
// 	backlog uint64,
// 	weights map[uint8]uint64,
// ) error {
// 	subStorage, err := ps.multiGasConstraints.Push()
// 	if err != nil {
// 		return fmt.Errorf("failed to push multi-gas constraint: %w", err)
// 	}

// 	constraint := OpenMultiGasConstraint(subStorage)
// 	if err := constraint.SetTarget(target); err != nil {
// 		return fmt.Errorf("failed to set target: %w", err)
// 	}
// 	if err := constraint.SetAdjustmentWindow(adjustmentWindow); err != nil {
// 		return fmt.Errorf("failed to set adjustment window: %w", err)
// 	}
// 	if err := constraint.SetBacklog(backlog); err != nil {
// 		return fmt.Errorf("failed to set backlog: %w", err)
// 	}
// 	if err := constraint.SetResourceWeights(weights); err != nil {
// 		return fmt.Errorf("failed to set resource weights: %w", err)
// 	}
// 	return nil
// }

// func (ps *L2PricingState) ClearMultiGasConstraints() error {
// 	length, err := ps.MultiGasConstraintsLength()
// 	if err != nil {
// 		return err
// 	}
// 	for range length {
// 		subStorage, err := ps.multiGasConstraints.Pop()
// 		if err != nil {
// 			return err
// 		}
// 		constraint := OpenMultiGasConstraint(subStorage)
// 		if err := constraint.Clear(); err != nil {
// 			return err
// 		}
// 	}
// 	return nil
// }
