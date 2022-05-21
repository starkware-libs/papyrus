

#[cfg(test)]
mod tests
{

    use crate::storage::create_storage;
    use crate::starknet::BlockNumber;

    #[test]
    fn test_add_block_number() {

        match create_storage() {
            Err(_e) => panic!("Could not create storage"),
            Ok(sh) => {
                let expected = BlockNumber{0 : 5};
                sh.set_latest_block_number(expected);
                assert_eq!(expected.0,5);
            }
        }



    }


}
