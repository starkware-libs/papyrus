use starknet::ContractAddress;

use snforge_std::{declare, ContractClassTrait, start_mock_call, CheatTarget};

use consensus::staking::IStakingDispatcher;
use consensus::staking::IStakingDispatcherTrait;

fn deploy_contract(name: ByteArray) -> ContractAddress {
    let contract = declare(name);
    contract.deploy(@ArrayTrait::new()).unwrap()
}

#[test]
fn test_get_validators() {
    let contract_address = deploy_contract("Staking");
    let dispatcher = IStakingDispatcher { contract_address };

    let mock_ret_val : Array<u32> = array![1, 2, 4, 3];
    start_mock_call(contract_address, selector!("get_validators"), mock_ret_val);
    assert(dispatcher.get_validators() == array![1, 2, 4, 3], 'Invalid validators');
}