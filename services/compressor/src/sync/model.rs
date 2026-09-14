use pallas::{crypto::hash::Hash, network::miniprotocols::chainsync::Tip};

pub type BlockSlot = u64;
pub type BlockHash = Hash<32>;
pub type RawBlock = Vec<u8>;

#[derive(Debug, Clone)]
pub enum PullEvent {
    RollForward(BlockSlot, BlockHash, RawBlock, Tip),
    RollBack(BlockSlot, BlockHash, Tip),
    RollBackOrigin(Tip),
}
