pub trait StorageSerde: Sized {
    fn serialize_into(&self, res: &mut impl std::io::Write);
    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self>;
}

pub trait StorageSerdeEx: StorageSerde {
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self>;
}
impl<T: StorageSerde> StorageSerdeEx for T {
    fn serialize(&self) -> Vec<u8> {
        let mut res: Vec<u8> = Vec::new();
        self.serialize_into(&mut res);
        res
    }
    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        let res = Self::deserialize_from(bytes)?;
        let mut buf = [0u8, 1];
        // Make sure we are at EOF.
        if bytes.read(&mut buf[..]).ok()? != 0 {
            return None;
        }
        Some(res)
    }
}
