#[starknet::interface]
pub trait IStaking<TContractState> {
    // List of current validator IDs.
    fn get_validators(ref self: TContractState) -> Array<felt252>;
    fn expect_validators(ref self: TContractState, validators: Array<felt252>);
    fn mirror(self: @TContractState, arr: Array<felt252>) -> Array<felt252>;
    fn return_result(self: @TContractState, num: felt252) -> felt252;
}

#[starknet::contract]
mod Staking {
    use core::traits::TryInto;
    use core::option::OptionTrait;
    use core::traits::Into;
    use core::array::ArrayTrait;
    use starknet::{ContractAddress, get_caller_address, storage_access::StorageBaseAddress};


    #[storage]
    struct Storage {
        // List of the current validator to return.
        //
        // Cairo doesn't support storing arrays. Therefore we need to fake this by turning the array
        // of validators into a LegacyMap. Each key is just the index in the array, and the total
        // number of entrants is stored in `num_validators`. A result of this is that we must set
        // each expectation before calling, and we cannot set a list of expectations.
        validators: LegacyMap<felt252, felt252>,
        num_validators: felt252
    }

    #[abi(embed_v0)]
    impl Staking of super::IStaking<ContractState> {
        // List of current validator IDs.
        fn get_validators(ref self: ContractState) -> Array<felt252> {
            println!("get_validators: num_validators={:?}", self.num_validators.read());
            let mut out = ArrayTrait::new();
            let mut i = 0;
            while i != self.num_validators.read() {
                out.append(self.validators.read(i));
                i += 1;
            };
            out
        }

        fn expect_validators(ref self: ContractState, mut validators: Array<felt252>) {
            println!("expect_validators: validators={:?}", validators);
            let mut count = 0;
            while validators.len() != 0 {
                match validators.pop_front() {
                    Option::Some(value) => {
                        self.validators.write(count, value);
                    },
                    Option::None => {},
                }
                count += 1;
            };
            self.num_validators.write(count);
        }

        fn mirror(self: @ContractState, arr: Array<felt252>) -> Array<felt252> {
            println!("The value of x is: {:?}", arr);
            assert(arr == array![1, 2], 'Input mismatch');
            assert(*arr.at(0) == 1, 'Input mismatch');
            arr
        }

        fn return_result(self: @ContractState, num: felt252) -> felt252 {
            num + 1
        }
    }
}