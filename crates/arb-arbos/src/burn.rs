// // Copyright 2021-2026, Offchain Labs, Inc.
// // For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

// package burn

// import (
// 	"fmt"

// 	"github.com/ethereum/go-ethereum/arbitrum/multigas"
// 	"github.com/ethereum/go-ethereum/log"

// 	"github.com/offchainlabs/nitro/arbos/util"
// )

// type Burner interface {
// 	Burn(kind multigas.ResourceKind, amount uint64) error
// 	BurnMultiGas(amount multigas.MultiGas) error
// 	Burned() uint64
// 	GasLeft() uint64 // `SystemBurner`s panic (no notion of GasLeft)
// 	BurnOut() error
// 	Restrict(err error)
// 	HandleError(err error) error
// 	ReadOnly() bool
// 	TracingInfo() *util.TracingInfo
// }

use arbitrum::multigas::resources::{MultiGas, ResourceKind};

use crate::util::util::TracingInfo;

pub trait Burner {
    fn burn(&mut self, kind: ResourceKind, amount: u64) -> Result<(), String>;
    fn burn_multi_gas(&mut self, amount: MultiGas) -> Result<(), String>;
    fn burned(&self) -> u64;
    /// Returns the gas left. `SystemBurner` panics — it has no notion of a gas limit.
    fn gas_left(&self) -> u64;
    /// Returns `Err` if gas is exhausted. `SystemBurner` panics — it has no gas limit.
    fn burn_out(&self) -> Result<(), String>;
    fn restrict(&self, err: Option<&str>);
    fn handle_error(&self, err: &str) -> String;
    fn read_only(&self) -> bool;
    fn tracing_info(&self) -> Option<&TracingInfo>;
}

pub struct SystemBurner {
    gas_burnt: MultiGas,
    tracing_info: Option<TracingInfo>,
    read_only: bool,
}

// type SystemBurner struct {
// 	gasBurnt    multigas.MultiGas
// 	tracingInfo *util.TracingInfo
// 	readOnly    bool
// }

impl SystemBurner {
    pub fn new(tracing_info: Option<TracingInfo>, read_only: bool) -> Self {
        Self {
            gas_burnt: MultiGas::default(),
            tracing_info,
            read_only,
        }
    }
}

impl Burner for SystemBurner {
    fn burn(&mut self, kind: ResourceKind, amount: u64) -> Result<(), String> {
        self.gas_burnt.saturating_increment_into(kind, amount);
        Ok(())
    }

    fn burn_multi_gas(&mut self, amount: MultiGas) -> Result<(), String> {
        self.gas_burnt.saturating_add_into(&amount);
        Ok(())
    }

    fn burned(&self) -> u64 {
        self.gas_burnt.single_gas()
    }

    fn gas_left(&self) -> u64 {
        panic!("called gas_left on a SystemBurner")
    }

    fn burn_out(&self) -> Result<(), String> {
        panic!("called burn_out on a SystemBurner")
    }

    fn restrict(&self, err: Option<&str>) {
        if let Some(e) = err {
            eprintln!("Restrict() received an error: {e}");
        }
    }

    fn handle_error(&self, err: &str) -> String {
        panic!("fatal error in system burner: {err}")
    }

    fn read_only(&self) -> bool {
        self.read_only
    }

    fn tracing_info(&self) -> Option<&TracingInfo> {
        self.tracing_info.as_ref()
    }
}

// func NewSystemBurner(tracingInfo *util.TracingInfo, readOnly bool) *SystemBurner {
// 	return &SystemBurner{
// 		tracingInfo: tracingInfo,
// 		readOnly:    readOnly,
// 	}
// }

// func (burner *SystemBurner) Burn(kind multigas.ResourceKind, amount uint64) error {
// 	burner.gasBurnt.SaturatingIncrementInto(kind, amount)
// 	return nil
// }

// func (burner *SystemBurner) BurnMultiGas(amount multigas.MultiGas) error {
// 	burner.gasBurnt.SaturatingAddInto(amount)
// 	return nil
// }

// func (burner *SystemBurner) Burned() uint64 {
// 	return burner.gasBurnt.SingleGas()
// }

// func (burner *SystemBurner) BurnOut() error {
// 	panic("called BurnOut on a system burner")
// }

// func (burner *SystemBurner) GasLeft() uint64 {
// 	panic("called GasLeft on a system burner")
// }

// func (burner *SystemBurner) Restrict(err error) {
// 	if err != nil {
// 		log.Error("Restrict() received an error", "err", err)
// 	}
// }

// func (burner *SystemBurner) HandleError(err error) error {
// 	panic(fmt.Sprintf("fatal error in system burner: %v", err))
// }

// func (burner *SystemBurner) ReadOnly() bool {
// 	return burner.readOnly
// }

// func (burner *SystemBurner) TracingInfo() *util.TracingInfo {
// 	return burner.tracingInfo
// }
