use crate::types::ConsensusContext;

// This should cause compilation to fail if the traits are not object safe.
#[test]
fn check_object_safety() {
    struct _Blk {}

    // This test only checks object safety, not that `ConsensusContext::Block` implements the
    // required traits. That check is performed by the compiler in an `impl ConsensusContext`
    // block.
    fn _check_context() -> Box<dyn ConsensusContext<Block = _Blk>> {
        todo!()
    }
}
