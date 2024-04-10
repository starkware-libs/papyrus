#[starknet::interface]
pub trait IStaking<TContractState> {
    // List of current validator IDs.
    fn get_validators(self: @TContractState) -> Array<u32>;
    fn return_result(self: @TContractState, num: felt252) -> felt252;
}

#[starknet::contract]
mod Staking {
    #[storage]
    struct Storage {}

    #[abi(embed_v0)]
    impl Staking of super::IStaking<ContractState> {
        // List of current validator IDs.
        fn get_validators(self: @ContractState) -> Array<u32> {
            ArrayTrait::new()
        }

        fn return_result(self: @ContractState, num: felt252) -> felt252 {
            9
        }
    }
}