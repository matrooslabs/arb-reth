use revm::context::ContextTr;

pub mod address_set;
pub mod arbosstate;
pub mod burn;
pub mod l1pricing;
pub mod l2pricing;
pub mod storage;
pub mod util;
pub trait ArbContextTr: ContextTr {}
