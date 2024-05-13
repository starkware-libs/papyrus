//! This file primarily contains the Context trait, which is the primary interface for the consensus
//! module to access the rest of the node. It also contains the ConsensusBlock trait, which is used
//! to keep the consensus module indifferent to the specific makeup of a block.
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

/// NodeId is used to to identify the node by consensus.
/// 1. This ID is derived from the id registered with Starknet's L2 staking contract.
/// 2. We must be able to derive the public key associated with this ID for the sake of validating
///    signatures.
///
/// TODO: Determine the actual type of NodeId.
pub type NodeId = u64;

/// In principle Consensus does not care about the content of a block. In practice though it will
/// need to perform certain activities with blocks:
/// 1. All proposals for a given height are held by consensus for book keeping, with only the
///    decided block returned to Context.
/// 2. If we re-propose a block, then Consensus needs the ability to stream out the content.
/// 3. We need the ability to identify the content of a proposal.
pub trait ConsensusBlock {
    /// Why do we use an associated type?
    /// 1. In principle consensus is indifferent to the actual content in a proposal, and so we
    ///    don't want consensus to be tied to a specific concrete type.
    /// 2. Why not make `proposal_stream` generic? While this would allow a given concrete block
    ///    type to stream its content in multiple forms, this would also make `ConsensusBlock`
    ///    object unsafe, which hurts ergonomics.
    type StreamT;

    /// Identifies the block for the sake of Consensus voting.
    ///
    /// The proposal's round must not be included in the ID, as, beyond being a detail of
    /// consensus, Tendermint must be able to progress a value across multiple rounds of a given
    /// height. Including a proposal's height in ID is optional from the perspective of consensus.
    /// Since the proposal as well as votes sign not only on the block ID but also the height at
    /// which they vote, not including height poses no security risk. Including it has no impact on
    /// Tendermint.
    fn id(&self) -> u64;

    /// This Iterator returns chunks of the block's content which are streamed out to peers for them
    /// to validate the proposal.
    fn proposal_stream(&self) -> impl Iterator<Item = Self::StreamT>;
}

/// Interface for consensus to call out to the node.
///
/// Why `Send + Sync`?
/// 1. We expect multiple components within consensus to concurrently access the context.
/// 2. The other option is for each component to have its own copy (i.e. clone) of the context, but
///    this is object unsafe.
/// 3. Given that we see the context as basically a connector to other components in the node, the
///    limitation of Sync to keep functions `&self` shouldn't be a problem.
#[async_trait]
pub trait Context: Send + Sync {
    /// See `ConsensusBlock::StreamT` for why we use an associated type.
    type BlockT: ConsensusBlock;

    /// This function is called by consensus to request a block from the node. It expects that this
    /// call will return immediately and that consensus can then stream in the block's content in
    /// parallel to the block being built.
    ///
    /// Params:
    /// - `height`: The height of the block to be built. Specifically this indicates the initial
    ///   state of the block.
    ///
    /// Returns:
    /// - A receiver for the stream of the block's content.
    /// - A receiver for the fully built block once Context has finished streaming out the content
    ///   and building it. If the block fails to be built, the Sender will be dropped by Context. If
    ///   this proposal is the decision made then this block will be returned to Context. Otherwise
    ///   it will be dropped by consensus upon reaching a different decision.
    async fn build_block(
        &self,
        height: u64,
    ) -> (mpsc::Receiver<<Self::BlockT as ConsensusBlock>::StreamT>, oneshot::Receiver<Self::BlockT>);

    /// This function is called by consensus to validate a block. It expects that this call will
    /// return immediately and that consensus can then stream in the block's content in parallel to
    /// consensus continuing to handle other tasks.
    ///
    /// Params:
    /// - `height`: The height of the block to be built. Specifically this indicates the initial
    ///   state of the block.
    /// - A receiver for the stream of the block's content.
    ///
    /// Returns:
    /// - A receiver for the fully built block once it is fully validated. If the block fails to be
    ///   built, the Sender will be dropped by Context. If this proposal is the decision made then
    ///   this block will be returned to Context. Otherwise it will be dropped by consensus upon
    ///   reaching a different decision.
    async fn validate_block(
        &self,
        height: u64,
        content: mpsc::Receiver<<Self::BlockT as ConsensusBlock>::StreamT>,
    ) -> oneshot::Receiver<Self::BlockT>;

    /// Get the set of validators for a given height. These are the nodes that can propose and vote
    /// on blocks.
    async fn validators(&self, height: u64) -> Vec<NodeId>;

    /// Calculates the ID of the Proposer based on the inputs.
    fn proposer(&self, validators: Vec<NodeId>, height: u64) -> NodeId;
}

// Check object safety.
type _Blk = Box<dyn ConsensusBlock<StreamT = u32>>;
type _Ctx = Box<dyn Context<BlockT = _Blk>>;
