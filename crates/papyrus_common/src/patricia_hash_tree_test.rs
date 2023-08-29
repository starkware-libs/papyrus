use starknet_api::hash::StarkFelt;

use super::calculate_root;

#[test]
fn test_patricia() {
    let root =
        calculate_root(vec![StarkFelt::from(1_u8), StarkFelt::from(2_u8), StarkFelt::from(3_u8)]);
    let expected_root =
        StarkFelt::try_from("0x231e110514ca3a27707cd6c365e00685142d43b03d26f6274db51cbfa91aa1c")
            .unwrap();
    assert_eq!(root, expected_root);
}

#[test]
fn test_edge_patricia() {
    let root = calculate_root(vec![StarkFelt::from(1_u8)]);
    let expected_root =
        StarkFelt::try_from("0x268a9d47dde48af4b6e2c33932ed1c13adec25555abaa837c376af4ea2f8ad4")
            .unwrap();
    assert_eq!(root, expected_root);
}

#[test]
fn test_binary_patricia() {
    let root = calculate_root(vec![StarkFelt::from(1_u8), StarkFelt::from(2_u8)]);
    let expected_root =
        StarkFelt::try_from("0x599927f1181d5633c6f680dbf039534de49c44e0b9903c5305b2582dfd6a56a")
            .unwrap();
    assert_eq!(root, expected_root);
}
