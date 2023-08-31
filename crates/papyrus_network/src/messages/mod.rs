pub mod block;
pub mod common;

pub mod proto {
    pub mod p2p {
        pub mod proto {
            include!(concat!(env!("OUT_DIR"), "/_.rs"));
        }
    }
}
