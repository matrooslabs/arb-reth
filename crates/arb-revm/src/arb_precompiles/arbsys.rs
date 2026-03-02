//! ArbSys precompile — system-level functionality for interacting with L1 and
//! understanding the call stack.
//!
//! Precompile address: 0x0000000000000000000000000000000000000064

use std::sync::OnceLock;

use revm::{
    context_interface::{Block, Cfg, ContextTr, JournalTr},
    precompile::{PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::{keccak256, Address, Bytes, B256, U256},
};

/// ArbSys precompile address (0x64).
pub const ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x64,
]);

// ────────────────────────────────────────────────────────────────────────────
// L1 ↔ L2 address aliasing
// ────────────────────────────────────────────────────────────────────────────

/// The offset added to L1 contract addresses to produce their L2 alias:
/// `0x1111000000000000000000000000000000001111`.
const ADDRESS_ALIAS_OFFSET: [u8; 20] = [
    0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x11,
];

/// Applies the L1-to-L2 address alias (wrapping add of the offset).
pub fn apply_l1_to_l2_alias(addr: Address) -> Address {
    let mut bytes: [u8; 20] = addr.into();
    let mut carry: u16 = 0;
    for i in (0..20).rev() {
        let sum = bytes[i] as u16 + ADDRESS_ALIAS_OFFSET[i] as u16 + carry;
        bytes[i] = sum as u8;
        carry = sum >> 8;
    }
    Address::new(bytes)
}

/// Reverses the L1-to-L2 address alias (wrapping sub of the offset).
pub fn undo_l1_to_l2_alias(addr: Address) -> Address {
    let mut bytes: [u8; 20] = addr.into();
    let mut borrow: i16 = 0;
    for i in (0..20).rev() {
        let diff = bytes[i] as i16 - ADDRESS_ALIAS_OFFSET[i] as i16 - borrow;
        bytes[i] = diff as u8;
        borrow = if diff < 0 { 1 } else { 0 };
    }
    Address::new(bytes)
}

// ────────────────────────────────────────────────────────────────────────────
// Function selectors
// ────────────────────────────────────────────────────────────────────────────

fn make_selector(sig: &str) -> [u8; 4] {
    let h = keccak256(sig.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

struct Selectors {
    arb_block_number: [u8; 4],
    arb_block_hash: [u8; 4],
    arb_chain_id: [u8; 4],
    arb_os_version: [u8; 4],
    get_storage_gas_available: [u8; 4],
    is_top_level_call: [u8; 4],
    map_l1_sender_to_l2_alias: [u8; 4],
    was_my_callers_aliased: [u8; 4],
    my_callers_without_aliasing: [u8; 4],
    send_tx_to_l1: [u8; 4],
    send_merkle_tree_state: [u8; 4],
    withdraw_eth: [u8; 4],
}

static SELECTORS: OnceLock<Selectors> = OnceLock::new();

fn selectors() -> &'static Selectors {
    SELECTORS.get_or_init(|| Selectors {
        arb_block_number: make_selector("arbBlockNumber()"),
        arb_block_hash: make_selector("arbBlockHash(uint256)"),
        arb_chain_id: make_selector("arbChainID()"),
        arb_os_version: make_selector("arbOSVersion()"),
        get_storage_gas_available: make_selector("getStorageGasAvailable()"),
        is_top_level_call: make_selector("isTopLevelCall()"),
        map_l1_sender_to_l2_alias: make_selector(
            "mapL1SenderContractAddressToL2Alias(address,address)",
        ),
        was_my_callers_aliased: make_selector("wasMyCallersAddressAliased()"),
        my_callers_without_aliasing: make_selector("myCallersAddressWithoutAliasing()"),
        send_tx_to_l1: make_selector("sendTxToL1(address,bytes)"),
        send_merkle_tree_state: make_selector("sendMerkleTreeState()"),
        withdraw_eth: make_selector("withdrawEth(address)"),
    })
}

// ────────────────────────────────────────────────────────────────────────────
// ABI helpers
// ────────────────────────────────────────────────────────────────────────────

fn decode_u256(data: &[u8]) -> Result<U256, PrecompileError> {
    if data.len() < 32 {
        return Err(PrecompileError::Other("invalid input length".into()));
    }
    Ok(U256::from_be_slice(&data[..32]))
}

fn decode_address(data: &[u8]) -> Result<Address, PrecompileError> {
    if data.len() < 32 {
        return Err(PrecompileError::Other("invalid input length".into()));
    }
    // ABI-encoded address: 12 zero padding bytes followed by 20 address bytes
    Ok(Address::from_slice(&data[12..32]))
}

fn encode_u256(val: U256) -> Bytes {
    Bytes::from(val.to_be_bytes::<32>().to_vec())
}

fn encode_bool(val: bool) -> Bytes {
    let mut out = [0u8; 32];
    out[31] = val as u8;
    Bytes::from(out.to_vec())
}

fn encode_b256(val: B256) -> Bytes {
    Bytes::from(val.0.to_vec())
}

fn encode_address(addr: Address) -> Bytes {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(addr.as_slice());
    Bytes::from(out.to_vec())
}

/// Extracts a u64 from a U256, returning an error if the value is too large.
fn u256_to_u64(val: U256) -> Result<u64, PrecompileError> {
    let limbs = val.as_limbs();
    if limbs[1] != 0 || limbs[2] != 0 || limbs[3] != 0 {
        return Err(PrecompileError::Other("block number out of u64 range".into()));
    }
    Ok(limbs[0])
}

// ────────────────────────────────────────────────────────────────────────────
// Dispatch
// ────────────────────────────────────────────────────────────────────────────

/// Runs the ArbSys precompile, dispatching on the 4-byte function selector.
pub fn run<CTX: ContextTr>(ctx: &mut CTX, input: &[u8], gas_limit: u64) -> PrecompileResult {
    if input.len() < 4 {
        return Err(PrecompileError::Other("calldata too short".into()));
    }
    let (sel_bytes, args) = input.split_at(4);
    let sel: [u8; 4] = sel_bytes.try_into().unwrap();
    let s = selectors();

    if sel == s.arb_block_number {
        arb_block_number(ctx, gas_limit)
    } else if sel == s.arb_block_hash {
        arb_block_hash(ctx, args, gas_limit)
    } else if sel == s.arb_chain_id {
        arb_chain_id(ctx, gas_limit)
    } else if sel == s.arb_os_version {
        arb_os_version(gas_limit)
    } else if sel == s.get_storage_gas_available {
        get_storage_gas_available(gas_limit)
    } else if sel == s.is_top_level_call {
        is_top_level_call(ctx, gas_limit)
    } else if sel == s.map_l1_sender_to_l2_alias {
        map_l1_sender_to_l2_alias(args, gas_limit)
    } else if sel == s.was_my_callers_aliased {
        was_my_callers_aliased()
    } else if sel == s.my_callers_without_aliasing {
        my_callers_without_aliasing()
    } else if sel == s.send_tx_to_l1 {
        send_tx_to_l1(ctx, args, gas_limit)
    } else if sel == s.send_merkle_tree_state {
        send_merkle_tree_state()
    } else if sel == s.withdraw_eth {
        withdraw_eth(ctx, args, gas_limit)
    } else {
        Err(PrecompileError::Other("unknown selector".into()))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Function implementations
// ────────────────────────────────────────────────────────────────────────────

/// Gets the current L2 block number.
fn arb_block_number<CTX: ContextTr>(ctx: &mut CTX, gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(PrecompileOutput::new(GAS, encode_u256(ctx.block().number())))
}

/// Gets the L2 block hash for a given block number, if it falls within the last
/// 256 blocks. Mirrors the EVM `BLOCKHASH` window constraint.
fn arb_block_hash<CTX: ContextTr>(
    ctx: &mut CTX,
    args: &[u8],
    gas_limit: u64,
) -> PrecompileResult {
    const GAS: u64 = 20;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    let requested = decode_u256(args)?;
    let current = ctx.block().number();

    if requested >= current || current - requested > U256::from(256u64) {
        return Err(PrecompileError::Other("invalid block number for ArbBlockHash".into()));
    }

    let req_u64 = u256_to_u64(requested)?;
    let hash = ctx.block_hash(req_u64).unwrap_or(B256::ZERO);
    Ok(PrecompileOutput::new(GAS, encode_b256(hash)))
}

/// Gets the rollup's unique chain identifier.
fn arb_chain_id<CTX: ContextTr>(ctx: &mut CTX, gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(PrecompileOutput::new(GAS, encode_u256(U256::from(ctx.cfg().chain_id()))))
}

/// Gets the current ArbOS version.
///
/// The Go implementation returns `55 + c.State.ArbOSVersion()` (Nitro starts at
/// version 56). ArbOS state is not reachable via standard [`ContextTr`]; this
/// returns `0` as a placeholder. Wire in ArbOS state to return the real value.
fn arb_os_version(gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(PrecompileOutput::new(GAS, encode_u256(U256::ZERO)))
}

/// Returns 0 — Nitro has no concept of storage gas.
fn get_storage_gas_available(gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(PrecompileOutput::new(GAS, encode_u256(U256::ZERO)))
}

/// Checks if the call is top-level (deprecated).
///
/// Returns `true` when the EVM call depth is ≤ 2, meaning the ArbSys caller
/// was invoked directly by the transaction (depth 1) or from one contract deep
/// (depth 2, where the precompile call itself adds one more level).
fn is_top_level_call<CTX: ContextTr>(ctx: &mut CTX, gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    let top_level = ctx.journal().depth() <= 2;
    Ok(PrecompileOutput::new(GAS, encode_bool(top_level)))
}

/// Returns the L2 alias of the given L1 sender address.
///
/// The `dest` argument is accepted for ABI compatibility but is unused,
/// mirroring the Go implementation.
fn map_l1_sender_to_l2_alias(args: &[u8], gas_limit: u64) -> PrecompileResult {
    const GAS: u64 = 0;
    if gas_limit < GAS {
        return Err(PrecompileError::OutOfGas);
    }
    let sender = decode_address(args)?;
    // `dest` (args[32..64]) is present for ABI compatibility but unused
    Ok(PrecompileOutput::new(GAS, encode_address(apply_l1_to_l2_alias(sender))))
}

/// Checks whether the caller's caller used an L1→L2 aliased address.
///
/// Requires ArbOS tx-processor state (call stack, top tx type) which is not
/// accessible through standard [`ContextTr`].
fn was_my_callers_aliased() -> PrecompileResult {
    Err(PrecompileError::Other(
        "wasMyCallersAddressAliased requires ArbOS tx-processor state".into(),
    ))
}

/// Returns the caller's caller address with any L1→L2 aliasing undone.
///
/// Requires ArbOS tx-processor state (call stack) which is not accessible
/// through standard [`ContextTr`].
fn my_callers_without_aliasing() -> PrecompileResult {
    Err(PrecompileError::Other(
        "myCallersAddressWithoutAliasing requires ArbOS tx-processor state".into(),
    ))
}

/// Appends a message to the L1 outbox Merkle tree and burns the attached value.
///
/// Requires ArbOS state (outbox Merkle accumulator, balance burning, event
/// emission) which is not accessible through standard [`ContextTr`].
fn send_tx_to_l1<CTX: ContextTr>(
    _ctx: &mut CTX,
    _args: &[u8],
    _gas_limit: u64,
) -> PrecompileResult {
    Err(PrecompileError::Other(
        "sendTxToL1 requires ArbOS state (outbox Merkle accumulator)".into(),
    ))
}

/// Returns the root, size, and partials of the outbox Merkle tree.
///
/// Callable only by address zero. Requires ArbOS state.
fn send_merkle_tree_state() -> PrecompileResult {
    Err(PrecompileError::Other(
        "sendMerkleTreeState requires ArbOS state (Merkle accumulator)".into(),
    ))
}

/// Sends the attached ETH value to `destination` on L1.
///
/// Convenience wrapper around `sendTxToL1` with empty calldata.
fn withdraw_eth<CTX: ContextTr>(ctx: &mut CTX, args: &[u8], gas_limit: u64) -> PrecompileResult {
    // destination is the first arg; delegate with empty calldataForL1
    let _destination = decode_address(args)?;
    send_tx_to_l1(ctx, args, gas_limit)
}
