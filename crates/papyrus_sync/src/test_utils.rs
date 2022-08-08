use std::cmp::Eq;
use std::collections::HashSet;
use std::hash::Hash;

use serde::Serialize;

pub fn assert_eq_without_order<T: IntoIterator>(seq1: T, seq2: T)
where
    T::Item: Hash,
    T::Item: Eq,
    T::IntoIter: ExactSizeIterator,
{
    assert_eq_without_order_iters(seq1.into_iter(), seq2.into_iter());
}

pub fn assert_eq_without_order_serializable<T: IntoIterator>(seq1: T, seq2: T)
where
    T::Item: Serialize,
    T::IntoIter: ExactSizeIterator,
{
    let string_seq1 = seq1.into_iter().map(|x| serde_json::to_string(&x).unwrap());
    let string_seq2 = seq2.into_iter().map(|x| serde_json::to_string(&x).unwrap());
    assert_eq_without_order_iters(string_seq1, string_seq2);
}

fn assert_eq_without_order_iters<
    T: ExactSizeIterator,
    S: ExactSizeIterator<Item = <T as Iterator>::Item>,
>(
    iter1: T,
    iter2: S,
) where
    <T as Iterator>::Item: Hash,
    <T as Iterator>::Item: Eq,
{
    assert_eq!(iter1.len(), iter2.len());
    let set2 = HashSet::<S::Item>::from_iter(iter2);
    for x in iter1 {
        assert!(set2.contains(&x));
    }
}
