use core::fmt;

// package multigas

// import (
// 	"encoding/json"
// 	"fmt"
// 	"io"
// 	"math"
// 	"math/bits"

// 	"github.com/ethereum/go-ethereum/common/hexutil"
// 	"github.com/ethereum/go-ethereum/rlp"
// )

// // ResourceKind represents a dimension for the multi-dimensional gas.
// type ResourceKind uint8

// //go:generate stringer -type=ResourceKind -trimprefix=ResourceKind
// const (
// 	ResourceKindUnknown ResourceKind = iota
// 	ResourceKindComputation
// 	ResourceKindHistoryGrowth
// 	ResourceKindStorageAccess
// 	ResourceKindStorageGrowth
// 	ResourceKindL1Calldata
// 	ResourceKindL2Calldata
// 	ResourceKindWasmComputation
// 	NumResourceKind
// )

/// A dimension for multi-dimensional gas accounting.
///
/// Discriminants match Go's `iota` ordering so `as u8` / `as usize` indexing
/// into `MultiGas::gas` is valid.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Unknown         = 0,
    Computation     = 1,
    HistoryGrowth   = 2,
    StorageAccess   = 3,
    StorageGrowth   = 4,
    L1Calldata      = 5,
    L2Calldata      = 6,
    WasmComputation = 7,
}

impl ResourceKind {
    /// Number of resource kinds (= `NumResourceKind` in Go).
    pub const COUNT: usize = 8;
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Unknown         => "Unknown",
            Self::Computation     => "Computation",
            Self::HistoryGrowth   => "HistoryGrowth",
            Self::StorageAccess   => "StorageAccess",
            Self::StorageGrowth   => "StorageGrowth",
            Self::L1Calldata      => "L1Calldata",
            Self::L2Calldata      => "L2Calldata",
            Self::WasmComputation => "WasmComputation",
        };
        f.write_str(s)
    }
}

// // CheckResourceKind checks whether the given id is a valid resource.
// func CheckResourceKind(id uint8) (ResourceKind, error) {
// 	if id <= uint8(ResourceKindUnknown) || id >= uint8(NumResourceKind) {
// 		return ResourceKindUnknown, fmt.Errorf("invalid resource id: %v", id)
// 	}
// 	return ResourceKind(id), nil
// }

/// Returns `Err` if `id` is 0 (`Unknown`) or out of range.
///
/// Maps to Go's `CheckResourceKind`.
pub fn check_resource_kind(id: u8) -> Result<ResourceKind, String> {
    if id == 0 || id as usize >= ResourceKind::COUNT {
        return Err(format!("invalid resource id: {}", id));
    }
    // SAFETY: id is in 1..=7, all of which are valid ResourceKind discriminants.
    Ok(unsafe { core::mem::transmute::<u8, ResourceKind>(id) })
}

// // MultiGas tracks gas usage across multiple resource kinds, while also
// // maintaining a single-dimensional total gas sum and refund amount.
// type MultiGas struct {
// 	gas    [NumResourceKind]uint64
// 	total  uint64
// 	refund uint64
// }

// // Pair represents a single resource kind and its associated gas amount.
// type Pair struct {
// 	Kind   ResourceKind
// 	Amount uint64
// }

// // ZeroGas creates a MultiGas value with all fields set to zero.
// func ZeroGas() MultiGas {
// 	return MultiGas{}
// }

/// A resource kind and its associated gas amount.
///
/// Maps to Go's `Pair`.
pub struct Pair {
    pub kind:   ResourceKind,
    pub amount: u64,
}

/// Gas usage tracked across all resource dimensions plus a single-dimensional
/// total and an SSTORE refund counter.
///
/// Maps to Go's `MultiGas`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MultiGas {
    gas:    [u64; ResourceKind::COUNT],
    total:  u64,
    refund: u64,
}

// // NewMultiGas creates a new MultiGas with the given resource kind initialized to `amount`.
// // All other kinds are zero. The total is also set to `amount`.
// func NewMultiGas(kind ResourceKind, amount uint64) MultiGas {
// 	var mg MultiGas
// 	mg.gas[kind] = amount
// 	mg.total = amount
// 	return mg
// }

// // MultiGasFromPairs creates a new MultiGas from resource–amount pairs.
// // Intended for constant-like construction; panics on overflow.
// func MultiGasFromPairs(pairs ...Pair) MultiGas {
// 	var mg MultiGas
// 	for _, p := range pairs {
// 		mg.gas[p.Kind] = p.Amount
// 	}
// 	if mg.recomputeTotal() {
// 		panic("multigas overflow")
// 	}
// 	return mg
// }

// // ComputationGas returns a MultiGas initialized with computation gas.
// func ComputationGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindComputation, amount)
// }

// // HistoryGrowthGas returns a MultiGas initialized with history growth gas.
// func HistoryGrowthGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindHistoryGrowth, amount)
// }

// // StorageAccessGas returns a MultiGas initialized with storage access gas.
// func StorageAccessGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindStorageAccess, amount)
// }

// // StorageGrowthGas returns a MultiGas initialized with storage growth gas.
// func StorageGrowthGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindStorageGrowth, amount)
// }

// // L1CalldataGas returns a MultiGas initialized with L1 calldata gas.
// func L1CalldataGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindL1Calldata, amount)
// }

// // L2CalldataGas returns a MultiGas initialized with L2 calldata gas.
// func L2CalldataGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindL2Calldata, amount)
// }

// // WasmComputationGas returns a MultiGas initialized with computation gas used for WASM (Stylus contracts).
// func WasmComputationGas(amount uint64) MultiGas {
// 	return NewMultiGas(ResourceKindWasmComputation, amount)
// }

impl MultiGas {
    /// All fields zero. Maps to Go's `ZeroGas()`.
    pub fn zero() -> Self {
        Self::default()
    }

    /// Initialises `kind` to `amount`; total is also set to `amount`.
    ///
    /// Maps to Go's `NewMultiGas`.
    pub fn new(kind: ResourceKind, amount: u64) -> Self {
        let mut mg = Self::default();
        mg.gas[kind as usize] = amount;
        mg.total = amount;
        mg
    }

    /// Builds from resource–amount pairs; panics on overflow.
    ///
    /// Maps to Go's `MultiGasFromPairs`.
    pub fn from_pairs(pairs: &[Pair]) -> Self {
        let mut mg = Self::default();
        for p in pairs {
            mg.gas[p.kind as usize] = p.amount;
        }
        if mg.recompute_total() {
            panic!("multigas overflow");
        }
        mg
    }

    pub fn computation(amount: u64)      -> Self { Self::new(ResourceKind::Computation,     amount) }
    pub fn history_growth(amount: u64)   -> Self { Self::new(ResourceKind::HistoryGrowth,   amount) }
    pub fn storage_access(amount: u64)   -> Self { Self::new(ResourceKind::StorageAccess,   amount) }
    pub fn storage_growth(amount: u64)   -> Self { Self::new(ResourceKind::StorageGrowth,   amount) }
    pub fn l1_calldata(amount: u64)      -> Self { Self::new(ResourceKind::L1Calldata,      amount) }
    pub fn l2_calldata(amount: u64)      -> Self { Self::new(ResourceKind::L2Calldata,      amount) }
    pub fn wasm_computation(amount: u64) -> Self { Self::new(ResourceKind::WasmComputation, amount) }

    /// Returns the gas amount for `kind`. Maps to Go's `MultiGas.Get`.
    pub fn get(&self, kind: ResourceKind) -> u64 {
        self.gas[kind as usize]
    }

    /// Returns a copy with `kind` set to `amount`, total adjusted.
    /// Returns `(updated, true)` on overflow. Maps to Go's `MultiGas.With`.
    pub fn with(&self, kind: ResourceKind, amount: u64) -> (Self, bool) {
        let mut res = *self;
        let base = self.total.wrapping_sub(self.gas[kind as usize]);
        let (new_total, overflow) = saturating_scalar_add(base, amount);
        if overflow {
            return (*self, true);
        }
        res.total = new_total;
        res.gas[kind as usize] = amount;
        (res, false)
    }

    /// Returns the SSTORE refund. Maps to Go's `MultiGas.GetRefund`.
    pub fn get_refund(&self) -> u64 {
        self.refund
    }

    /// Returns a copy with `refund` set to `amount`. Maps to Go's `MultiGas.WithRefund`.
    pub fn with_refund(&self, amount: u64) -> Self {
        let mut res = *self;
        res.refund = amount;
        res
    }

    /// Returns the single-dimensional total minus the refund.
    ///
    /// Maps to Go's `MultiGas.SingleGas`.
    pub fn single_gas(&self) -> u64 {
        self.total.saturating_sub(self.refund)
    }

    pub fn is_zero(&self) -> bool {
        self.total == 0 && self.refund == 0 && self.gas == [0u64; ResourceKind::COUNT]
    }

    /// Adds `x` per-kind, total, and refund. Returns `(sum, true)` on overflow.
    ///
    /// Maps to Go's `MultiGas.SafeAdd`.
    pub fn safe_add(&self, x: &Self) -> (Self, bool) {
        let mut res = *self;
        for i in 0..ResourceKind::COUNT {
            let (v, overflow) = saturating_scalar_add(res.gas[i], x.gas[i]);
            if overflow { return (*self, true); }
            res.gas[i] = v;
        }
        let (total, overflow) = saturating_scalar_add(res.total, x.total);
        if overflow { return (*self, true); }
        res.total = total;
        let (refund, overflow) = saturating_scalar_add(res.refund, x.refund);
        if overflow { return (*self, true); }
        res.refund = refund;
        (res, false)
    }

    /// Adds `x` saturating on overflow. Maps to Go's `MultiGas.SaturatingAdd`.
    pub fn saturating_add(&self, x: &Self) -> Self {
        let mut res = *self;
        for i in 0..ResourceKind::COUNT {
            res.gas[i] = res.gas[i].saturating_add(x.gas[i]);
        }
        res.total  = res.total.saturating_add(x.total);
        res.refund = res.refund.saturating_add(x.refund);
        res
    }

    /// Adds `x` into `self` in place, saturating. Maps to Go's `MultiGas.SaturatingAddInto`.
    pub fn saturating_add_into(&mut self, x: &Self) {
        for i in 0..ResourceKind::COUNT {
            self.gas[i] = self.gas[i].saturating_add(x.gas[i]);
        }
        self.total  = self.total.saturating_add(x.total);
        self.refund = self.refund.saturating_add(x.refund);
    }

    /// Subtracts `x` per-kind and refund, recomputes total. Returns `(diff, true)` on underflow.
    ///
    /// Maps to Go's `MultiGas.SafeSub`.
    pub fn safe_sub(&self, x: &Self) -> (Self, bool) {
        let mut res = *self;
        for i in 0..ResourceKind::COUNT {
            let (v, underflow) = saturating_scalar_sub(res.gas[i], x.gas[i]);
            if underflow { return (*self, true); }
            res.gas[i] = v;
        }
        let (refund, underflow) = saturating_scalar_sub(res.refund, x.refund);
        if underflow { return (*self, true); }
        res.refund = refund;
        res.recompute_total();
        (res, false)
    }

    /// Subtracts `x` saturating on underflow. Maps to Go's `MultiGas.SaturatingSub`.
    pub fn saturating_sub(&self, x: &Self) -> Self {
        let mut res = *self;
        for i in 0..ResourceKind::COUNT {
            res.gas[i] = res.gas[i].saturating_sub(x.gas[i]);
        }
        res.refund = res.refund.saturating_sub(x.refund);
        res.recompute_total();
        res
    }

    /// Increments `kind` and total by `gas`. Returns `(updated, true)` on overflow.
    ///
    /// Maps to Go's `MultiGas.SafeIncrement`.
    pub fn safe_increment(&self, kind: ResourceKind, gas: u64) -> (Self, bool) {
        let mut res = *self;
        let (v, overflow) = saturating_scalar_add(self.gas[kind as usize], gas);
        if overflow { return (*self, true); }
        res.gas[kind as usize] = v;
        let (total, overflow) = saturating_scalar_add(self.total, gas);
        if overflow { return (*self, true); }
        res.total = total;
        (res, false)
    }

    /// Increments `kind` and total saturating. Maps to Go's `MultiGas.SaturatingIncrement`.
    pub fn saturating_increment(&self, kind: ResourceKind, gas: u64) -> Self {
        let mut res = *self;
        res.gas[kind as usize] = res.gas[kind as usize].saturating_add(gas);
        res.total = res.total.saturating_add(gas);
        res
    }

    /// Decrements `kind` and total saturating (clamped to 0). Maps to Go's `MultiGas.SaturatingDecrement`.
    pub fn saturating_decrement(&self, kind: ResourceKind, gas: u64) -> Self {
        let mut res = *self;
        let current = res.gas[kind as usize];
        let reduced = gas.min(current);
        res.gas[kind as usize] = current - reduced;
        res.total = res.total.saturating_sub(reduced);
        res
    }

    /// Increments `kind` and total in place, saturating. Maps to Go's `MultiGas.SaturatingIncrementInto`.
    pub fn saturating_increment_into(&mut self, kind: ResourceKind, gas: u64) {
        self.gas[kind as usize] = self.gas[kind as usize].saturating_add(gas);
        self.total = self.total.saturating_add(gas);
    }

    /// Recomputes `total` from per-kind amounts. Returns `true` on overflow.
    fn recompute_total(&mut self) -> bool {
        self.total = 0;
        for i in 0..ResourceKind::COUNT {
            let (sum, overflow) = saturating_scalar_add(self.total, self.gas[i]);
            if overflow {
                self.total = u64::MAX;
                return true;
            }
            self.total = sum;
        }
        false
    }
}

/// Adds two `u64` values, returning `(sum, overflowed)`.
/// On overflow, sum is `u64::MAX`. Maps to Go's `saturatingScalarAdd`.
fn saturating_scalar_add(a: u64, b: u64) -> (u64, bool) {
    let (sum, overflow) = a.overflowing_add(b);
    if overflow { (u64::MAX, true) } else { (sum, false) }
}

/// Subtracts two `u64` values, returning `(diff, underflowed)`.
/// On underflow, diff is `0`. Maps to Go's `saturatingScalarSub`.
fn saturating_scalar_sub(a: u64, b: u64) -> (u64, bool) {
    if b > a { (0, true) } else { (a - b, false) }
}

// // Get returns the gas amount for the specified resource kind.
// func (z MultiGas) Get(kind ResourceKind) uint64 {
// 	return z.gas[kind]
// }

// // With returns a copy of z with the given resource kind set to amount.
// // The total is adjusted accordingly. It returns the updated value and true if an overflow occurred.
// func (z MultiGas) With(kind ResourceKind, amount uint64) (MultiGas, bool) {
// 	res, overflow := z, false

// 	res.total, overflow = saturatingScalarAdd(z.total-z.gas[kind], amount)
// 	if overflow {
// 		return z, true
// 	}

// 	res.gas[kind] = amount
// 	return res, false
// }

// // GetRefund gets the SSTORE refund computed at the end of the transaction.
// func (z MultiGas) GetRefund() uint64 {
// 	return z.refund
// }

// // WithRefund returns a copy of z with its refund set to amount.
// func (z MultiGas) WithRefund(amount uint64) MultiGas {
// 	res := z
// 	res.refund = amount
// 	return res
// }

// // SafeAdd returns a copy of z with the per-kind, total, and refund gas
// // added to the values from x. It returns the updated value and true if
// // an overflow occurred.
// func (z MultiGas) SafeAdd(x MultiGas) (MultiGas, bool) {
// 	res, overflow := z, false

// 	for i := 0; i < int(NumResourceKind); i++ {
// 		res.gas[i], overflow = saturatingScalarAdd(res.gas[i], x.gas[i])
// 		if overflow {
// 			return z, true
// 		}
// 	}

// 	res.total, overflow = saturatingScalarAdd(res.total, x.total)
// 	if overflow {
// 		return z, true
// 	}
// 	res.refund, overflow = saturatingScalarAdd(res.refund, x.refund)
// 	if overflow {
// 		return z, true
// 	}

// 	return res, false
// }

// // SaturatingAdd returns a copy of z with the per-kind, total, and refund gas
// // added to the values from x. On overflow, the affected field(s) are clamped
// // to MaxUint64.
// func (z MultiGas) SaturatingAdd(x MultiGas) MultiGas {
// 	res := z

// 	for i := 0; i < int(NumResourceKind); i++ {
// 		res.gas[i], _ = saturatingScalarAdd(res.gas[i], x.gas[i])
// 	}

// 	res.total, _ = saturatingScalarAdd(res.total, x.total)
// 	res.refund, _ = saturatingScalarAdd(res.refund, x.refund)
// 	return res
// }

// // SaturatingAddInto adds x into z in place (per kind, total, and refund).
// // On overflow, the affected field(s) are clamped to MaxUint64.
// // This is a hot-path helper; the public immutable API remains preferred elsewhere.
// func (z *MultiGas) SaturatingAddInto(x MultiGas) {
// 	for i := 0; i < int(NumResourceKind); i++ {
// 		z.gas[i], _ = saturatingScalarAdd(z.gas[i], x.gas[i])
// 	}
// 	z.total, _ = saturatingScalarAdd(z.total, x.total)
// 	z.refund, _ = saturatingScalarAdd(z.refund, x.refund)
// }

// // SafeSub returns a copy of z with the per-kind, total, and refund gas
// // subtracted by the values from x. It returns the updated value and true if
// // a underflow occurred.
// func (z MultiGas) SafeSub(x MultiGas) (MultiGas, bool) {
// 	res, underflow := z, false

// 	for i := 0; i < int(NumResourceKind); i++ {
// 		res.gas[i], underflow = saturatingScalarSub(res.gas[i], x.gas[i])
// 		if underflow {
// 			return z, true
// 		}
// 	}

// 	res.refund, underflow = saturatingScalarSub(res.refund, x.refund)
// 	if underflow {
// 		return z, true
// 	}

// 	res.recomputeTotal()

// 	return res, false
// }

// // SaturatingSub returns a copy of z with the per-kind, total, and refund gas
// // subtracted by the values from x. On underflow, the affected field(s) are
// // clamped to zero.
// func (z MultiGas) SaturatingSub(x MultiGas) MultiGas {
// 	res := z
// 	for i := 0; i < int(NumResourceKind); i++ {
// 		res.gas[i], _ = saturatingScalarSub(res.gas[i], x.gas[i])
// 	}
// 	res.refund, _ = saturatingScalarSub(res.refund, x.refund)
// 	res.recomputeTotal()
// 	return res
// }

// // SafeIncrement returns a copy of z with the given resource kind
// // and the total incremented by gas. It returns the updated value and true if
// // an overflow occurred.
// func (z MultiGas) SafeIncrement(kind ResourceKind, gas uint64) (MultiGas, bool) {
// 	res, overflow := z, false

// 	res.gas[kind], overflow = saturatingScalarAdd(z.gas[kind], gas)
// 	if overflow {
// 		return z, true
// 	}

// 	res.total, overflow = saturatingScalarAdd(z.total, gas)
// 	if overflow {
// 		return z, true
// 	}

// 	return res, false
// }

// // SaturatingIncrement returns a copy of z with the given resource kind
// // and the total incremented by gas. On overflow, the field(s) are clamped to MaxUint64.
// func (z MultiGas) SaturatingIncrement(kind ResourceKind, gas uint64) MultiGas {
// 	res := z
// 	res.gas[kind], _ = saturatingScalarAdd(z.gas[kind], gas)
// 	res.total, _ = saturatingScalarAdd(z.total, gas)
// 	return res
// }

// // SaturatingDecrement returns a copy of z with the given resource kind
// // and the total decremented by gas. On underflow, the field(s) are clamped to 0.
// func (z MultiGas) SaturatingDecrement(kind ResourceKind, gas uint64) MultiGas {
// 	res := z

// 	current := res.gas[kind]
// 	var reduced uint64
// 	if current < gas {
// 		reduced = current
// 		res.gas[kind] = 0
// 	} else {
// 		reduced = gas
// 		res.gas[kind] = current - gas
// 	}

// 	if res.total < reduced {
// 		res.total = 0
// 	} else {
// 		res.total -= reduced
// 	}

// 	return res
// }

// // SaturatingIncrementInto increments the given resource kind and the total
// // in place by gas. On overflow, the affected field(s) are clamped to MaxUint64.
// // Unlike SaturatingIncrement, this method mutates the receiver directly and
// // is intended for VM hot paths where avoiding value copies is critical.
// func (z *MultiGas) SaturatingIncrementInto(kind ResourceKind, gas uint64) {
// 	z.gas[kind], _ = saturatingScalarAdd(z.gas[kind], gas)
// 	z.total, _ = saturatingScalarAdd(z.total, gas)
// }

// // SingleGas returns the single-dimensional total gas.
// func (z MultiGas) SingleGas() uint64 {
// 	return z.total - z.refund
// }

// func (z MultiGas) IsZero() bool {
// 	return z.total == 0 && z.refund == 0 && z.gas == [NumResourceKind]uint64{}
// }

// // multiGasJSON is an auxiliary type for JSON marshaling/unmarshaling of MultiGas.
// type multiGasJSON struct {
// 	Unknown         hexutil.Uint64 `json:"unknown"`
// 	Computation     hexutil.Uint64 `json:"computation"`
// 	HistoryGrowth   hexutil.Uint64 `json:"historyGrowth"`
// 	StorageAccess   hexutil.Uint64 `json:"storageAccess"`
// 	StorageGrowth   hexutil.Uint64 `json:"storageGrowth"`
// 	L1Calldata      hexutil.Uint64 `json:"l1Calldata"`
// 	L2Calldata      hexutil.Uint64 `json:"l2Calldata"`
// 	WasmComputation hexutil.Uint64 `json:"wasmComputation"`
// 	Refund          hexutil.Uint64 `json:"refund"`
// 	Total           hexutil.Uint64 `json:"total"`
// }

// // MarshalJSON implements json.Marshaler for MultiGas.
// func (z MultiGas) MarshalJSON() ([]byte, error) {
// 	return json.Marshal(multiGasJSON{
// 		Unknown:         hexutil.Uint64(z.gas[ResourceKindUnknown]),
// 		Computation:     hexutil.Uint64(z.gas[ResourceKindComputation]),
// 		HistoryGrowth:   hexutil.Uint64(z.gas[ResourceKindHistoryGrowth]),
// 		StorageAccess:   hexutil.Uint64(z.gas[ResourceKindStorageAccess]),
// 		StorageGrowth:   hexutil.Uint64(z.gas[ResourceKindStorageGrowth]),
// 		L1Calldata:      hexutil.Uint64(z.gas[ResourceKindL1Calldata]),
// 		L2Calldata:      hexutil.Uint64(z.gas[ResourceKindL2Calldata]),
// 		WasmComputation: hexutil.Uint64(z.gas[ResourceKindWasmComputation]),
// 		Refund:          hexutil.Uint64(z.refund),
// 		Total:           hexutil.Uint64(z.total),
// 	})
// }

// // UnmarshalJSON implements json.Unmarshaler for MultiGas.
// func (z *MultiGas) UnmarshalJSON(data []byte) error {
// 	var j multiGasJSON
// 	if err := json.Unmarshal(data, &j); err != nil {
// 		return err
// 	}
// 	*z = ZeroGas()
// 	z.gas[ResourceKindUnknown] = uint64(j.Unknown)
// 	z.gas[ResourceKindComputation] = uint64(j.Computation)
// 	z.gas[ResourceKindHistoryGrowth] = uint64(j.HistoryGrowth)
// 	z.gas[ResourceKindStorageAccess] = uint64(j.StorageAccess)
// 	z.gas[ResourceKindStorageGrowth] = uint64(j.StorageGrowth)
// 	z.gas[ResourceKindL1Calldata] = uint64(j.L1Calldata)
// 	z.gas[ResourceKindL2Calldata] = uint64(j.L2Calldata)
// 	z.gas[ResourceKindWasmComputation] = uint64(j.WasmComputation)
// 	z.refund = uint64(j.Refund)
// 	z.total = uint64(j.Total)
// 	return nil
// }

// // EncodeRLP encodes MultiGas as:
// // [ total, refund, gas[0], gas[1], ..., gas[NumResourceKind-1] ]
// func (z *MultiGas) EncodeRLP(w io.Writer) error {
// 	enc := rlp.NewEncoderBuffer(w)
// 	l := enc.List()

// 	enc.WriteUint64(z.total)
// 	enc.WriteUint64(z.refund)
// 	for i := 0; i < int(NumResourceKind); i++ {
// 		enc.WriteUint64(z.gas[i])
// 	}

// 	enc.ListEnd(l)
// 	return enc.Flush()
// }

// // DecodeRLP decodes MultiGas in a forward/backward-compatible way.
// // Extra per-dimension entries are skipped; missing ones are treated as zero.
// func (z *MultiGas) DecodeRLP(s *rlp.Stream) error {
// 	if _, err := s.List(); err != nil {
// 		return err
// 	}

// 	total, err := s.Uint64()
// 	if err != nil {
// 		return err
// 	}
// 	refund, err := s.Uint64()
// 	if err != nil {
// 		return err
// 	}

// 	for i := 0; ; i++ {
// 		val, err := s.Uint64()
// 		if err == rlp.EOL {
// 			break // end of list
// 		}
// 		if err != nil {
// 			return err
// 		}
// 		if i < int(NumResourceKind) {
// 			z.gas[i] = val
// 		}
// 		// if i >= NumResourceKind, just skip extra lines
// 	}

// 	if err := s.ListEnd(); err != nil {
// 		return err
// 	}

// 	z.total = total
// 	z.refund = refund
// 	return nil
// }

// // recomputeTotal recomputes the total gas from the per-kind amounts. Returns
// // true if an overflow occurred (and the total was set to MaxUint64).
// func (z *MultiGas) recomputeTotal() (overflow bool) {
// 	z.total = 0
// 	for i := 0; i < int(NumResourceKind); i++ {
// 		z.total, overflow = saturatingScalarAdd(z.total, z.gas[i])
// 		if overflow {
// 			return
// 		}
// 	}
// 	return
// }

// // saturatingScalarAdd adds two uint64 values, returning the sum and a boolean
// // indicating whether an overflow occurred. If an overflow occurs, the sum is
// // set to math.MaxUint64.
// func saturatingScalarAdd(a, b uint64) (uint64, bool) {
// 	sum, carry := bits.Add64(a, b, 0)
// 	if carry != 0 {
// 		return math.MaxUint64, true
// 	}
// 	return sum, false
// }

// // saturatingScalarSub subtracts two uint64 values, returning the difference and a boolean
// // indicating whether an underflow occurred. If an underflow occurs, the difference is
// // set to 0.
// func saturatingScalarSub(a, b uint64) (uint64, bool) {
// 	diff, borrow := bits.Sub64(a, b, 0)
// 	if borrow != 0 {
// 		return 0, true
// 	}
// 	return diff, false
// }
