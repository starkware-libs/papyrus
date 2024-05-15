#[cfg(test)]
#[path = "types_test.rs"]
mod types_test;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

/// Used to identify the node by consensus.
/// 1. This ID is derived from the id registered with Starknet's L2 staking contract.
/// 2. We must be able to derive the public key associated with this ID for the sake of validating
///    signatures.
// TODO: Determine the actual type of NodeId.
pub type NodeId = u64;

/// Interface that any concrete block type must implement to be used by consensus.
// In principle Consensus does not care about the content of a block. In practice though it will
// need to perform certain activities with blocks:
// 1. All proposals for a given height are held by consensus for book keeping, with only the decided
//    block returned to ConsensusContext.
// 2. If we re-propose a block, then Consensus needs the ability to stream out the content.
// 3. We need the ability to identify the content of a proposal.
pub trait ConsensusBlock {
    /// The chunks of content returned for each step when streaming out the proposal.
    // Why do we use an associated type?
    // 1. In principle consensus is indifferent to the actual content in a proposal, and so we don't
    //    want consensus to be tied to a specific concrete type.
    // 2. Why not make `proposal_stream` generic? While this would allow a given concrete block type
    //    to stream its content in multiple forms, this would also make `ConsensusBlock` object
    //    unsafe, which hurts ergonomics.
    type ProposalChunk;
    /// Iterator for accessing the proposal's content.
    // ProposalIter is used instead of returning `impl Iterator` due to object safety.
    type ProposalIter: Iterator<Item = Self::ProposalChunk>;

    /// Identifies the block for the sake of Consensus voting.
    // The proposal's round must not be included in the ID, as, beyond being a detail of
    // consensus, Tendermint must be able to progress a value across multiple rounds of a given
    // height.
    //
    // Including a proposal's height in ID is optional from the perspective of consensus.
    // Since the proposal as well as votes sign not only on the block ID but also the height at
    // which they vote, not including height poses no security risk. Including it has no impact on
    // Tendermint.
    fn id(&self) -> u64;

    /// Returns an interator over the block's content. Since this is the proposal content we stream
    /// out only the information used to build the proposal need be included, not the built
    /// information (ie the transactions, not the state diff).
    // For milestone 1 this will be used by Peering when we receive a proposal, to fake streaming
    // this into the ConsensusContext.
    fn proposal_stream(&self) -> Self::ProposalIter;
}

/// Interface for consensus to call out to the node.
// Why `Send + Sync`?
// 1. We expect multiple components within consensus to concurrently access the context.
// 2. The other option is for each component to have its own copy (i.e. clone) of the context, but
//    this is object unsafe (Clone requires Sized).
// 3. Given that we see the context as basically a connector to other components in the node, the
//    limitation of Sync to keep functions `&self` shouldn't be a problem.
#[async_trait]
pub trait ConsensusContext: Send + Sync {
    // See [`ConsensusBlock::StreamT`] for why we use an associated type.
    type Block: ConsensusBlock;

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
    /// - A receiver for the fully built block once ConsensusContext has finished streaming out the
    ///   content and building it. If the block fails to be built, the Sender will be dropped by
    ///   ConsensusContext.
    async fn build_proposal(
        &self,
        height: u64,
    ) -> (
        mpsc::Receiver<<Self::Block as ConsensusBlock>::ProposalChunk>,
        oneshot::Receiver<Self::Block>,
    );

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
    /// - A receiver for the fully built block. If a valid block cannot be built the Sender will be
    ///   dropped by ConsensusContext.
    async fn validate_proposal(
        &self,
        height: u64,
        content: mpsc::Receiver<<Self::Block as ConsensusBlock>::ProposalChunk>,
    ) -> oneshot::Receiver<Self::Block>;

    /// Get the set of validators for a given height. These are the nodes that can propose and vote
    /// on blocks.
    // TODO: We expect this to change in the future to BTreeMap. Why?
    // 1. Map - The nodes will have associated information (e.g. voting weight).
    // 2. BTreeMap - We want a stable ordering of the nodes for deterministic leader selection.
    async fn validators(&self, height: u64) -> Vec<NodeId>;

    /// Calculates the ID of the Proposer based on the inputs.
    fn proposer(&self, validators: &Vec<NodeId>, height: u64) -> NodeId;
}
