//! Patricia hash tree implementation.
//!
//! Supports root hash calculation for Stark felt values, keyed by consecutive 64 bits numbers,
//! starting from 0.
//!
//! Each edge is marked with one or more bits.
//! The key of a node is the concatenation of the edges' marks in the path from the root to this
//! node.
//! The input keys are in the leaves, and each leaf is an input key.
//!
//! The edges coming out of an internal node with a key `K` are:
//! - If there are input keys that start with 'K0...' and 'K1...', then two edges come out, marked
//! with '0' and '1' bits.
//! - Otherwise, a single edge mark with 'Z' is coming out. 'Z' is the longest string, such that all
//! the input keys that start with 'K...' start with 'KZ...' as well. Note, the order of the input
//! keys in this implementation forces 'Z' to be a zeros string.
//!
//! Hash of a node depends on the number of edges coming out of it:
//! - A leaf: The hash is the input value of its key.
//! - A single edge: Pedersen::hash(child_hash, edge_mark) + edge_length.
//! - '0' and '1' edges: Pedersen::hash(zero_child_hash, one_child_hash).

#[cfg(test)]
#[path = "patricia_hash_tree_test.rs"]
mod patricia_hash_tree_test;

use bitvec::prelude::{BitArray, Msb0};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::transaction_hash::ZERO;

const TREE_HEIGHT: u8 = 64;
type BitPath = BitArray<[u8; 8], Msb0>;

// An entry in a Patricia tree.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    key: BitPath,
    value: Felt,
}

// A sub-tree is defined by a sub-sequence of leaves with a common ancestor at the specified height,
// with no other leaves under it besides these.
#[derive(Debug)]
struct SubTree<'a> {
    leaves: &'a [Entry],
    // Levels from the root.
    height: u8,
}

enum SubTreeSplitting {
    // Number of '0' bits that all the keys start with.
    CommonZerosPrefix(u8),
    // The index of the first key that starts with a '1' bit.
    PartitionPoint(usize),
}

/// Calculates Patricia hash root on the given values.
/// The values are keyed by consecutive numbers, starting from 0.
pub fn calculate_root(values: Vec<Felt>) -> Felt {
    if values.is_empty() {
        return *ZERO;
    }
    let leaves: Vec<Entry> = values
        .into_iter()
        .zip(0u64..)
        .map(|(felt, idx)| Entry { key: idx.to_be_bytes().into(), value: felt })
        .collect();
    get_hash(SubTree { leaves: &leaves[..], height: 0_u8 })
}

// Recursive hash calculation. There are 3 cases:
// - Leaf: The sub tree height is maximal. It should contain exactly one entry.
// - Edge: All the keys start with a longest common ('0's) prefix. NOTE: We assume that the keys are
// a continuous range, and hence the case of '1's in the longest common prefix is impossible.
// - Binary: Some keys start with '0' bit and some start with '1' bit.
fn get_hash(sub_tree: SubTree<'_>) -> Felt {
    if sub_tree.height == TREE_HEIGHT {
        return sub_tree.leaves.first().expect("a leaf should not be empty").value;
    }
    match get_splitting(&sub_tree) {
        SubTreeSplitting::CommonZerosPrefix(n_zeros) => get_edge_hash(sub_tree, n_zeros),
        SubTreeSplitting::PartitionPoint(partition_point) => {
            get_binary_hash(sub_tree, partition_point)
        }
    }
}

// Hash on a '0's sequence with the bottom sub tree.
fn get_edge_hash(sub_tree: SubTree<'_>, n_zeros: u8) -> Felt {
    let child_hash =
        get_hash(SubTree { leaves: sub_tree.leaves, height: sub_tree.height + n_zeros });
    let child_and_path_hash = Pedersen::hash(&child_hash, &ZERO);
    child_and_path_hash + Felt::from(n_zeros)
}

// Hash on both sides: starts with '0' bit and starts with '1' bit.
// Assumes: 0 < partition point < sub_tree.len().
fn get_binary_hash(sub_tree: SubTree<'_>, partition_point: usize) -> Felt {
    let zero_hash = get_hash(SubTree {
        leaves: &sub_tree.leaves[..partition_point],
        height: sub_tree.height + 1,
    });
    let one_hash = get_hash(SubTree {
        leaves: &sub_tree.leaves[partition_point..],
        height: sub_tree.height + 1,
    });
    Pedersen::hash(&zero_hash, &one_hash)
}

// Returns the manner the keys of a subtree are splitting: some keys start with '1' or all keys
// start with '0'.
fn get_splitting(sub_tree: &SubTree<'_>) -> SubTreeSplitting {
    let mut height = sub_tree.height;

    let first_one_bit_index =
        sub_tree.leaves.partition_point(|entry| !entry.key[usize::from(height)]);
    if first_one_bit_index < sub_tree.leaves.len() {
        return SubTreeSplitting::PartitionPoint(first_one_bit_index);
    }

    height += 1;
    let mut n_zeros = 1;

    while height < TREE_HEIGHT {
        if sub_tree.leaves.last().expect("sub tree should not be empty").key[usize::from(height)] {
            break;
        }
        n_zeros += 1;
        height += 1;
    }
    SubTreeSplitting::CommonZerosPrefix(n_zeros)
}
