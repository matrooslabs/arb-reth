// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

use revm::{
    Database,
    context_interface::{ContextTr, JournalTr},
    primitives::{Address, I256, U256},
};

use crate::{
    burn::Burner,
    l1pricing::{
        batch_poster::OpenPosterResult,
        l1pricing::{L1PricingState, L1_PRICER_FUNDS_POOL_ADDRESS, UpdateSpendingError},
    },
};

const ARBOS_VERSION_2: u64 = 2;
const ARBOS_VERSION_3: u64 = 3;

impl<B: Burner> L1PricingState<B> {
    // func (ps *L1PricingState) _preVersion2_UpdateForBatchPosterSpending(
    //     statedb vm.StateDB, evm *vm.EVM,
    //     updateTime, currentTime uint64,
    //     batchPoster common.Address, weiSpent *big.Int,
    //     scenario util.TracingScenario,
    // ) error
    pub fn pre_version2_update_for_batch_poster_spending<CTX: ContextTr>(
        &mut self,
        ctx: &mut CTX,
        update_time: u64,
        current_time: u64,
        batch_poster: Address,
        wei_spent: U256,
    ) -> Result<(), <<CTX::Journal as JournalTr>::Database as Database>::Error>
    where
        B: Clone,
    {
        let mut poster_state = self
            .batch_poster_table
            .open_poster(ctx, batch_poster, true)
            .map_err(|e| match e {
                OpenPosterResult::Db(e) => e,
                OpenPosterResult::Semantic(e) => {
                    panic!("pre_version2: unexpected semantic error opening poster: {e:?}")
                }
            })?;

        // Compute previous shortfall (oldSurplus) before any changes.
        let total_funds_due = self.batch_poster_table.total_funds_due(ctx.db_mut())?;
        let mut funds_due_for_rewards = self.funds_due_for_rewards(ctx.db_mut())?;
        let pool_balance =
            ctx.journal_mut().load_account(L1_PRICER_FUNDS_POOL_ADDRESS)?.data.info.balance;
        let pool_balance_signed = I256::try_from(pool_balance).unwrap_or(I256::MAX);
        let old_surplus = pool_balance_signed
            .saturating_sub(total_funds_due.saturating_add(funds_due_for_rewards));

        // Compute allocation fraction: updateTimeDelta / timeDelta.
        let mut last_update_time = self.last_update_time(ctx.db_mut())?;
        if last_update_time == 0 && current_time > 0 {
            // First update — use updateTime - 1 as a synthetic prior timestamp.
            // Note: _preVersion2 guards on currentTime > 0, unlike v10 which guards on updateTime > 0.
            last_update_time = update_time.wrapping_sub(1);
        }
        if update_time >= current_time || update_time < last_update_time {
            return Ok(()); // historically returned an error; now a no-op
        }
        let allocation_numerator = update_time - last_update_time;
        let allocation_denominator = current_time - last_update_time;
        let (allocation_numerator, allocation_denominator) = if allocation_denominator == 0 {
            (1u64, 1u64)
        } else {
            (allocation_numerator, allocation_denominator)
        };

        // Allocate units (plain u64 multiply matching Go's uint64 arithmetic).
        let mut units_since_update = self.units_since_update(ctx.db_mut())?;
        let units_allocated = (units_since_update as u128
            * allocation_numerator as u128
            / allocation_denominator as u128) as u64;
        units_since_update -= units_allocated;
        self.set_units_since_update(ctx, units_since_update)?;

        // Update funds due to this batch poster.
        let due_to_poster = poster_state.funds_due(ctx.db_mut())?;
        let wei_spent_signed = I256::try_from(wei_spent).unwrap_or(I256::MAX);
        poster_state.set_funds_due(ctx, due_to_poster.saturating_add(wei_spent_signed))?;

        // Accrue rewards: fundsDueForRewards += unitsAllocated * perUnitReward.
        let per_unit_reward = self.per_unit_reward(ctx.db_mut())?;
        let units_i256 = I256::try_from(U256::from(units_allocated)).expect("u64 fits in I256");
        let per_unit_i256 = I256::try_from(U256::from(per_unit_reward)).expect("u64 fits in I256");
        funds_due_for_rewards =
            funds_due_for_rewards.saturating_add(units_i256.saturating_mul(per_unit_i256));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)?;

        // Allocate available funds = pool_balance * allocationNumerator / allocationDenominator.
        let pool_balance =
            ctx.journal_mut().load_account(L1_PRICER_FUNDS_POOL_ADDRESS)?.data.info.balance;
        let mut available_funds = pool_balance
            .saturating_mul(U256::from(allocation_numerator))
            / U256::from(allocation_denominator);

        // Pay rewards, as much as possible.
        let mut payment_for_rewards =
            U256::from(per_unit_reward).saturating_mul(U256::from(units_allocated));
        if available_funds < payment_for_rewards {
            payment_for_rewards = available_funds;
        }
        funds_due_for_rewards = funds_due_for_rewards
            .saturating_sub(I256::try_from(payment_for_rewards).unwrap_or(I256::MAX));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)?;
        let pay_rewards_to = self.pay_rewards_to(ctx.db_mut())?;
        if payment_for_rewards > U256::ZERO {
            if let Some(err) = ctx.journal_mut().transfer(
                L1_PRICER_FUNDS_POOL_ADDRESS,
                pay_rewards_to,
                payment_for_rewards,
            )? {
                panic!("pre_version2: rewards transfer failed: {err:?}");
            }
        }
        available_funds -= payment_for_rewards; // safe: available_funds >= payment_for_rewards

        // Settle up payments owed to ALL batch posters, as much as possible.
        let all_poster_addrs = self.batch_poster_table.all_posters(ctx.db_mut(), u64::MAX)?;
        for poster_addr in all_poster_addrs {
            let mut poster =
                self.batch_poster_table.open_poster(ctx, poster_addr, false).map_err(
                    |e| match e {
                        OpenPosterResult::Db(e) => e,
                        OpenPosterResult::Semantic(e) => panic!(
                            "pre_version2: unexpected error opening poster {poster_addr}: {e:?}"
                        ),
                    },
                )?;
            let balance_due = poster.funds_due(ctx.db_mut())?;
            if balance_due.is_positive() {
                let mut balance_to_transfer =
                    U256::try_from(balance_due).unwrap_or(U256::MAX);
                if available_funds < balance_to_transfer {
                    balance_to_transfer = available_funds;
                }
                if balance_to_transfer > U256::ZERO {
                    let addr_to_pay = poster.pay_to(ctx.db_mut())?;
                    if let Some(err) = ctx.journal_mut().transfer(
                        L1_PRICER_FUNDS_POOL_ADDRESS,
                        addr_to_pay,
                        balance_to_transfer,
                    )? {
                        panic!("pre_version2: poster payment transfer failed: {err:?}");
                    }
                    available_funds -= balance_to_transfer;
                    let new_balance_due =
                        balance_due - I256::try_from(balance_to_transfer).unwrap_or(I256::MAX);
                    poster.set_funds_due(ctx, new_balance_due)?;
                }
            }
        }

        // Update time.
        self.set_last_update_time(ctx, update_time)?;

        // Adjust price.
        if units_allocated > 0 {
            let total_funds_due = self.batch_poster_table.total_funds_due(ctx.db_mut())?;
            let funds_due_for_rewards = self.funds_due_for_rewards(ctx.db_mut())?;
            let pool_balance = ctx
                .journal_mut()
                .load_account(L1_PRICER_FUNDS_POOL_ADDRESS)?
                .data
                .info
                .balance;
            let pool_balance_signed = I256::try_from(pool_balance).unwrap_or(I256::MAX);
            let surplus = pool_balance_signed
                .saturating_sub(total_funds_due.saturating_add(funds_due_for_rewards));

            let inertia = self.inertia(ctx.db_mut())?;
            let equil_units = self.equilibration_units(ctx.db_mut())?;
            let equil_signed = I256::try_from(equil_units).unwrap_or(I256::MAX);
            let inertia_units = if inertia == 0 {
                I256::ZERO
            } else {
                equil_signed
                    .wrapping_div(I256::try_from(U256::from(inertia)).expect("u64 fits in I256"))
            };
            let price = self.price_per_unit(ctx.db_mut())?;
            let alloc_plus_inert = inertia_units.saturating_add(
                I256::try_from(U256::from(units_allocated)).expect("u64 fits in I256"),
            );

            // priceChange = (surplus*(equilUnits-1) - oldSurplus*equilUnits) / (equilUnits * allocPlusInert)
            let denom = equil_signed.saturating_mul(alloc_plus_inert);
            let price_change = if denom.is_zero() {
                I256::ZERO
            } else {
                let numer = surplus
                    .saturating_mul(equil_signed.saturating_sub(I256::ONE))
                    .saturating_sub(old_surplus.saturating_mul(equil_signed));
                numer.wrapping_div(denom)
            };

            let new_price = if price_change >= I256::ZERO {
                price.saturating_add(U256::try_from(price_change).unwrap_or(U256::MAX))
            } else {
                let abs_change =
                    U256::try_from(price_change.wrapping_neg()).unwrap_or(U256::MAX);
                price.saturating_sub(abs_change)
            };
            self.set_price_per_unit(ctx, new_price)?;
        }

        Ok(())
    }

    // func (ps *L1PricingState) _preversion10_UpdateForBatchPosterSpending(
    //     statedb vm.StateDB, evm *vm.EVM, arbosVersion uint64,
    //     updateTime, currentTime uint64, batchPoster common.Address,
    //     weiSpent *big.Int, l1Basefee *big.Int, scenario util.TracingScenario,
    // ) error
    pub fn pre_version10_update_for_batch_poster_spending<CTX: ContextTr>(
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
        if arbos_version < ARBOS_VERSION_2 {
            return self
                .pre_version2_update_for_batch_poster_spending(
                    ctx,
                    update_time,
                    current_time,
                    batch_poster,
                    wei_spent,
                )
                .map_err(UpdateSpendingError::Db);
        }

        let mut poster_state = self
            .batch_poster_table
            .open_poster(ctx, batch_poster, true)
            .map_err(|e| match e {
                OpenPosterResult::Db(e) => UpdateSpendingError::Db(e),
                OpenPosterResult::Semantic(e) => {
                    panic!("pre_version10: unexpected semantic error opening poster: {e:?}")
                }
            })?;

        let mut funds_due_for_rewards =
            self.funds_due_for_rewards(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;

        // Compute allocation fraction: updateTimeDelta / timeDelta.
        let mut last_update_time =
            self.last_update_time(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        if last_update_time == 0 && update_time > 0 {
            last_update_time = update_time - 1;
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

        // Allocate units (saturating multiply, matching Go's SaturatingUMul).
        let mut units_since_update =
            self.units_since_update(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        let units_allocated =
            units_since_update.saturating_mul(allocation_numerator) / allocation_denominator;
        units_since_update -= units_allocated;
        self.set_units_since_update(ctx, units_since_update).map_err(UpdateSpendingError::Db)?;

        // Impose amortized cost cap (arbos_version >= 3).
        if arbos_version >= ARBOS_VERSION_3 {
            let amortized_cost_cap_bips =
                self.amortized_cost_cap_bips(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            if amortized_cost_cap_bips != 0 {
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
        let units_i256 = I256::try_from(U256::from(units_allocated)).expect("u64 fits in I256");
        let per_unit_i256 = I256::try_from(U256::from(per_unit_reward)).expect("u64 fits in I256");
        funds_due_for_rewards =
            funds_due_for_rewards.saturating_add(units_i256.saturating_mul(per_unit_i256));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)
            .map_err(UpdateSpendingError::Db)?;

        // Pay rewards, as much as possible. Use raw EVM pool balance (not l1FeesAvailable).
        let mut payment_for_rewards =
            U256::from(per_unit_reward).saturating_mul(U256::from(units_allocated));
        let pool_balance = ctx
            .journal_mut()
            .load_account(L1_PRICER_FUNDS_POOL_ADDRESS)
            .map_err(UpdateSpendingError::Db)?
            .data
            .info
            .balance;
        if pool_balance < payment_for_rewards {
            payment_for_rewards = pool_balance;
        }
        funds_due_for_rewards = funds_due_for_rewards
            .saturating_sub(I256::try_from(payment_for_rewards).unwrap_or(I256::MAX));
        self.set_funds_due_for_rewards(ctx, funds_due_for_rewards)
            .map_err(UpdateSpendingError::Db)?;
        let pay_rewards_to =
            self.pay_rewards_to(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        if payment_for_rewards > U256::ZERO {
            if let Some(err) = ctx
                .journal_mut()
                .transfer(L1_PRICER_FUNDS_POOL_ADDRESS, pay_rewards_to, payment_for_rewards)
                .map_err(UpdateSpendingError::Db)?
            {
                panic!("pre_version10: rewards transfer failed: {err:?}");
            }
        }

        // Re-read pool balance after rewards transfer.
        let available_funds = ctx
            .journal_mut()
            .load_account(L1_PRICER_FUNDS_POOL_ADDRESS)
            .map_err(UpdateSpendingError::Db)?
            .data
            .info
            .balance;

        // Settle payments owed to the single batch poster, as much as possible.
        let balance_due_to_poster =
            poster_state.funds_due(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
        if balance_due_to_poster.is_positive() {
            let balance_due_u256 = U256::try_from(balance_due_to_poster).unwrap_or(U256::MAX);
            let balance_to_transfer = available_funds.min(balance_due_u256);
            if !balance_to_transfer.is_zero() {
                let addr_to_pay =
                    poster_state.pay_to(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
                if let Some(err) = ctx
                    .journal_mut()
                    .transfer(L1_PRICER_FUNDS_POOL_ADDRESS, addr_to_pay, balance_to_transfer)
                    .map_err(UpdateSpendingError::Db)?
                {
                    panic!("pre_version10: poster payment transfer failed: {err:?}");
                }
                let transferred_signed = I256::try_from(balance_to_transfer).unwrap_or(I256::MAX);
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

            // surplus = poolBalance − (totalFundsDue + fundsDueForRewards)
            let pool_balance = ctx
                .journal_mut()
                .load_account(L1_PRICER_FUNDS_POOL_ADDRESS)
                .map_err(UpdateSpendingError::Db)?
                .data
                .info
                .balance;
            let pool_balance_signed = I256::try_from(pool_balance).unwrap_or(I256::MAX);
            let surplus = pool_balance_signed
                .saturating_sub(total_funds_due.saturating_add(funds_due_for_rewards));

            let inertia = self.inertia(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            let equil_units =
                self.equilibration_units(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
            let price = self.price_per_unit(ctx.db_mut()).map_err(UpdateSpendingError::Db)?;
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
                let abs_change = U256::try_from(price_change.wrapping_neg()).unwrap_or(U256::MAX);
                price.saturating_sub(abs_change)
            };
            self.set_price_per_unit(ctx, new_price).map_err(UpdateSpendingError::Db)?;
        }

        Ok(())
    }
}

// package l1pricing

// import (
// 	"math"
// 	"math/big"

// 	"github.com/ethereum/go-ethereum/common"
// 	"github.com/ethereum/go-ethereum/core/tracing"
// 	"github.com/ethereum/go-ethereum/core/types"
// 	"github.com/ethereum/go-ethereum/core/vm"
// 	"github.com/ethereum/go-ethereum/params"

// 	"github.com/offchainlabs/nitro/arbos/util"
// 	"github.com/offchainlabs/nitro/util/arbmath"
// )

// func (ps *L1PricingState) _preversion10_UpdateForBatchPosterSpending(
// 	statedb vm.StateDB,
// 	evm *vm.EVM,
// 	arbosVersion uint64,
// 	updateTime, currentTime uint64,
// 	batchPoster common.Address,
// 	weiSpent *big.Int,
// 	l1Basefee *big.Int,
// 	scenario util.TracingScenario,
// ) error {
// 	if arbosVersion < params.ArbosVersion_2 {
// 		return ps._preVersion2_UpdateForBatchPosterSpending(statedb, evm, updateTime, currentTime, batchPoster, weiSpent, scenario)
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
// 	availableFunds := statedb.GetBalance(types.L1PricerFundsPoolAddress)
// 	if arbmath.BigLessThan(availableFunds.ToBig(), paymentForRewards) {
// 		paymentForRewards = availableFunds.ToBig()
// 	}
// 	fundsDueForRewards = arbmath.BigSub(fundsDueForRewards, paymentForRewards)
// 	if err := ps.SetFundsDueForRewards(fundsDueForRewards); err != nil {
// 		return err
// 	}
// 	payRewardsTo, err := ps.PayRewardsTo()
// 	if err != nil {
// 		return err
// 	}
// 	err = util.TransferBalance(
// 		&types.L1PricerFundsPoolAddress, &payRewardsTo, paymentForRewards, evm, scenario, tracing.BalanceChangeTransferBatchposterReward,
// 	)
// 	if err != nil {
// 		return err
// 	}
// 	availableFunds = statedb.GetBalance(types.L1PricerFundsPoolAddress)

// 	// settle up payments owed to the batch poster, as much as possible
// 	balanceDueToPoster, err := posterState.FundsDue()
// 	if err != nil {
// 		return err
// 	}
// 	balanceToTransfer := balanceDueToPoster
// 	if arbmath.BigLessThan(availableFunds.ToBig(), balanceToTransfer) {
// 		balanceToTransfer = availableFunds.ToBig()
// 	}
// 	if balanceToTransfer.Sign() > 0 {
// 		addrToPay, err := posterState.PayTo()
// 		if err != nil {
// 			return err
// 		}
// 		err = util.TransferBalance(
// 			&types.L1PricerFundsPoolAddress, &addrToPay, balanceToTransfer, evm, scenario, tracing.BalanceChangeTransferBatchposterRefund,
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
// 		surplus := arbmath.BigSub(statedb.GetBalance(types.L1PricerFundsPoolAddress).ToBig(), arbmath.BigAdd(totalFundsDue, fundsDueForRewards))

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

// func (ps *L1PricingState) _preVersion2_UpdateForBatchPosterSpending(
// 	statedb vm.StateDB,
// 	evm *vm.EVM,
// 	updateTime, currentTime uint64,
// 	batchPoster common.Address,
// 	weiSpent *big.Int,
// 	scenario util.TracingScenario,
// ) error {
// 	batchPosterTable := ps.BatchPosterTable()
// 	posterState, err := batchPosterTable.OpenPoster(batchPoster, true)
// 	if err != nil {
// 		return err
// 	}

// 	// compute previous shortfall
// 	totalFundsDue, err := batchPosterTable.TotalFundsDue()
// 	if err != nil {
// 		return err
// 	}
// 	fundsDueForRewards, err := ps.FundsDueForRewards()
// 	if err != nil {
// 		return err
// 	}
// 	oldSurplus := arbmath.BigSub(statedb.GetBalance(types.L1PricerFundsPoolAddress).ToBig(), arbmath.BigAdd(totalFundsDue, fundsDueForRewards))

// 	// compute allocation fraction -- will allocate updateTimeDelta/timeDelta fraction of units and funds to this update
// 	lastUpdateTime, err := ps.LastUpdateTime()
// 	if err != nil {
// 		return err
// 	}
// 	if lastUpdateTime == 0 && currentTime > 0 { // it's the first update, so there isn't a last update time
// 		lastUpdateTime = updateTime - 1
// 	}
// 	if updateTime >= currentTime || updateTime < lastUpdateTime {
// 		return nil // historically this returned an error
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
// 	unitsAllocated := unitsSinceUpdate * allocationNumerator / allocationDenominator
// 	unitsSinceUpdate -= unitsAllocated
// 	if err := ps.SetUnitsSinceUpdate(unitsSinceUpdate); err != nil {
// 		return err
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

// 	// allocate funds to this update
// 	collectedSinceUpdate := statedb.GetBalance(types.L1PricerFundsPoolAddress)
// 	availableFunds := arbmath.BigDivByUint(arbmath.BigMulByUint(collectedSinceUpdate.ToBig(), allocationNumerator), allocationDenominator)

// 	// pay rewards, as much as possible
// 	paymentForRewards := arbmath.BigMulByUint(arbmath.UintToBig(perUnitReward), unitsAllocated)
// 	if arbmath.BigLessThan(availableFunds, paymentForRewards) {
// 		paymentForRewards = availableFunds
// 	}
// 	fundsDueForRewards = arbmath.BigSub(fundsDueForRewards, paymentForRewards)
// 	if err := ps.SetFundsDueForRewards(fundsDueForRewards); err != nil {
// 		return err
// 	}
// 	payRewardsTo, err := ps.PayRewardsTo()
// 	if err != nil {
// 		return err
// 	}
// 	err = util.TransferBalance(
// 		&types.L1PricerFundsPoolAddress, &payRewardsTo, paymentForRewards, evm, scenario, tracing.BalanceChangeTransferBatchposterReward,
// 	)
// 	if err != nil {
// 		return err
// 	}
// 	availableFunds = arbmath.BigSub(availableFunds, paymentForRewards)

// 	// settle up our batch poster payments owed, as much as possible
// 	allPosterAddrs, err := batchPosterTable.AllPosters(math.MaxUint64)
// 	if err != nil {
// 		return err
// 	}
// 	for _, posterAddr := range allPosterAddrs {
// 		poster, err := batchPosterTable.OpenPoster(posterAddr, false)
// 		if err != nil {
// 			return err
// 		}
// 		balanceDueToPoster, err := poster.FundsDue()
// 		if err != nil {
// 			return err
// 		}
// 		balanceToTransfer := balanceDueToPoster
// 		if arbmath.BigLessThan(availableFunds, balanceToTransfer) {
// 			balanceToTransfer = availableFunds
// 		}
// 		if balanceToTransfer.Sign() > 0 {
// 			addrToPay, err := poster.PayTo()
// 			if err != nil {
// 				return err
// 			}
// 			err = util.TransferBalance(
// 				&types.L1PricerFundsPoolAddress, &addrToPay, balanceToTransfer, evm, scenario, tracing.BalanceChangeTransferBatchposterRefund,
// 			)
// 			if err != nil {
// 				return err
// 			}
// 			availableFunds = arbmath.BigSub(availableFunds, balanceToTransfer)
// 			balanceDueToPoster = arbmath.BigSub(balanceDueToPoster, balanceToTransfer)
// 			err = poster.SetFundsDue(balanceDueToPoster)
// 			if err != nil {
// 				return err
// 			}
// 		}
// 	}

// 	// update time
// 	if err := ps.SetLastUpdateTime(updateTime); err != nil {
// 		return err
// 	}

// 	// adjust the price
// 	if unitsAllocated > 0 {
// 		totalFundsDue, err = batchPosterTable.TotalFundsDue()
// 		if err != nil {
// 			return err
// 		}
// 		fundsDueForRewards, err = ps.FundsDueForRewards()
// 		if err != nil {
// 			return err
// 		}
// 		surplus := arbmath.BigSub(statedb.GetBalance(types.L1PricerFundsPoolAddress).ToBig(), arbmath.BigAdd(totalFundsDue, fundsDueForRewards))

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
// 		priceChange := arbmath.BigDiv(
// 			arbmath.BigSub(
// 				arbmath.BigMul(surplus, arbmath.BigSub(equilUnits, common.Big1)),
// 				arbmath.BigMul(oldSurplus, equilUnits),
// 			),
// 			arbmath.BigMul(equilUnits, allocPlusInert),
// 		)

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
