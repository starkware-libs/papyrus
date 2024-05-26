use crate::types::ConsensusContext;

// This should cause compilation to fail if `ConsensusContext` is not object safe. Note that
// `ConsensusBlock` need not be object safe for this to work.
#[test]
fn check_object_safety() {
    fn _check_context() -> Box<dyn ConsensusContext<Block = ()>> {
        todo!()
    }
}
