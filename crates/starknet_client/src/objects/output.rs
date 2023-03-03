pub mod transaction{
    
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
    #[serde(tag="fee_estimation")]
    pub struct FeeEstimate{
        pub overall_fee: u128,
        pub gas_price: u128,
        pub gas_usage: u128,
        pub unit: String
    }
}