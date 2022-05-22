

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
                let mut mut_sh = sh;
                mut_sh.set_latest_block_number(expected);

                let res = mut_sh.get_latest_block_number();
                assert_eq!(res.0,5);


            }
        }



    }


}
