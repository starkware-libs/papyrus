use crate::{shash, StarkHash};

#[test]
fn hash_macro() {
    assert_eq!(
        shash!("0x123"),
        StarkHash::new([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0x1, 0x23
        ])
        .unwrap()
    );
}

#[test]
fn hash_json_serde() {
    let hash = shash!("0x123");
    assert_eq!(hash, serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap());
}

#[test]
fn hash_serde() {
    fn enc_len(n_nibbles: usize) -> usize {
        match n_nibbles {
            0..=27 => n_nibbles / 2 + 1,
            28..=33 => 17,
            _ => 32,
        }
    }

    // 64 nibbles are invalid.
    for n_nibbles in 0..64 {
        let mut bytes = [0u8; 32];
        // Set all nibbles to 0xf.
        for i in 0..n_nibbles {
            bytes[31 - (i >> 1)] |= 15 << (4 * (i & 1));
        }
        let h = StarkHash::new(bytes).unwrap();
        let mut res = Vec::new();
        assert!(h.serialize(&mut res).is_ok());
        assert_eq!(res.len(), enc_len(n_nibbles));
        let mut reader = &res[..];
        let d = StarkHash::deserialize(&mut reader).unwrap();
        assert_eq!(bytes, d.0);
    }
}
