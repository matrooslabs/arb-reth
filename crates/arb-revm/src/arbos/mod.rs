use revm::context::ContextTr;

pub mod arbosstate;
pub mod burn;
pub mod l1pricing;
pub mod storage;
pub trait ArbContextTr: ContextTr {}
