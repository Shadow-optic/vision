//! Pure Merkle tree over BLAKE3 hashes. No I/O, no DB — fully unit-testable.
//!
//! Construction:
//!   leaf  = BLAKE3(DOMAIN || 0x00 || content)
//!   node  = BLAKE3(DOMAIN || 0x01 || left || right)
//! Leaves are the sorted canonical row hashes of the snapshotted tables. At
//! each level an odd trailing node is *promoted* unchanged to the next level
//! (never duplicated), so every tree shape is a deterministic function of the
//! sorted leaf list.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub const DOMAIN: &[u8] = b"VI-Transparency/v1";
pub const LEAF_PREFIX: u8 = 0x00;
pub const NODE_PREFIX: u8 = 0x01;

pub fn hash_leaf(content: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN);
    h.update(&[LEAF_PREFIX]);
    h.update(content);
    *h.finalize().as_bytes()
}

pub fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN);
    h.update(&[NODE_PREFIX]);
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

/// Which side of the running node the sibling hash sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofNode {
    pub hash: [u8; 32],
    pub sibling_side: Side,
}

/// Root of the tree over `leaves` (assumed already sorted). `None` for an
/// empty tree — there is no meaningful empty root.
pub fn merkle_root(leaves: &[[u8; 32]]) -> Option<[u8; 32]> {
    if leaves.is_empty() {
        return None;
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_node(&level[i], &level[i + 1]));
            } else {
                next.push(level[i]); // odd node promoted unchanged
            }
            i += 2;
        }
        level = next;
    }
    Some(level[0])
}

/// Audit path for the leaf at `index`. `None` if out of range.
pub fn prove(leaves: &[[u8; 32]], index: usize) -> Option<Vec<ProofNode>> {
    if index >= leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut pos = index;
    while level.len() > 1 {
        let n = level.len();
        if pos % 2 == 1 {
            path.push(ProofNode {
                hash: level[pos - 1],
                sibling_side: Side::Left,
            });
        } else if pos + 1 < n {
            path.push(ProofNode {
                hash: level[pos + 1],
                sibling_side: Side::Right,
            });
        }
        // else: odd trailing node promoted — no sibling at this level.
        let mut next = Vec::with_capacity(n.div_ceil(2));
        let mut i = 0;
        while i < n {
            if i + 1 < n {
                next.push(hash_node(&level[i], &level[i + 1]));
            } else {
                next.push(level[i]);
            }
            i += 2;
        }
        level = next;
        pos /= 2;
    }
    Some(path)
}

/// Recompute the root from a leaf and its audit path. Also cross-checks that
/// each sibling side is consistent with `leaf_index`/`tree_size`, so a proof
/// cannot be replayed against a different position.
pub fn verify(
    leaf: &[u8; 32],
    leaf_index: usize,
    tree_size: usize,
    path: &[ProofNode],
    root: &[u8; 32],
) -> bool {
    if tree_size == 0 || leaf_index >= tree_size {
        return false;
    }
    let mut acc = *leaf;
    let mut pos = leaf_index;
    let mut n = tree_size;
    let mut i = 0usize; // path cursor
    while n > 1 {
        // Sibling at this level? Odd nodes pair left; even nodes pair right
        // when one exists; an odd trailing node is promoted with no sibling.
        let expected = if pos % 2 == 1 {
            Some(Side::Left)
        } else if pos + 1 < n {
            Some(Side::Right)
        } else {
            None
        };
        if let Some(side) = expected {
            let Some(node) = path.get(i) else {
                return false; // path shorter than the tree requires
            };
            if node.sibling_side != side {
                return false; // proof replayed against a different position
            }
            acc = match side {
                Side::Left => hash_node(&node.hash, &acc),
                Side::Right => hash_node(&acc, &node.hash),
            };
            i += 1;
        }
        pos /= 2;
        n = n.div_ceil(2);
    }
    i == path.len() && acc == *root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(s: &str) -> [u8; 32] {
        hash_leaf(s.as_bytes())
    }

    #[test]
    fn known_vector_single_leaf() {
        // Tree of one leaf: root IS the leaf hash.
        let a = leaf("alpha");
        assert_eq!(merkle_root(&[a]), Some(a));
    }

    #[test]
    fn known_vector_two_leaves() {
        let a = leaf("alpha");
        let b = leaf("beta");
        // Independent reference computation.
        let mut h = blake3::Hasher::new();
        h.update(b"VI-Transparency/v1");
        h.update(&[NODE_PREFIX]);
        h.update(&a);
        h.update(&b);
        let expected = *h.finalize().as_bytes();
        assert_eq!(merkle_root(&[a, b]), Some(expected));
    }

    #[test]
    fn known_vector_leaf_domain_separation() {
        let mut h = blake3::Hasher::new();
        h.update(b"VI-Transparency/v1");
        h.update(&[LEAF_PREFIX]);
        h.update(b"alpha");
        assert_eq!(leaf("alpha"), *h.finalize().as_bytes());
        // Same bytes without the leaf prefix must not collide.
        let raw = blake3::hash(b"alpha");
        assert_ne!(leaf("alpha"), *raw.as_bytes());
    }

    #[test]
    fn odd_leaf_promoted_not_duplicated() {
        let a = leaf("a");
        let b = leaf("b");
        let c = leaf("c");
        // root = node(node(a,b), c) — promotion, NOT node(node(a,b), node(c,c)).
        let expected = hash_node(&hash_node(&a, &b), &c);
        assert_eq!(merkle_root(&[a, b, c]), Some(expected));
        let duplicated = hash_node(&hash_node(&a, &b), &hash_node(&c, &c));
        assert_ne!(merkle_root(&[a, b, c]), Some(duplicated));
    }

    #[test]
    fn empty_tree_has_no_root() {
        assert_eq!(merkle_root(&[]), None);
    }

    #[test]
    fn proof_round_trip_every_position() {
        for size in 1..=9usize {
            let leaves: Vec<[u8; 32]> =
                (0..size).map(|i| leaf(&format!("row-{i}"))).collect();
            let root = merkle_root(&leaves).unwrap();
            for index in 0..size {
                let path = prove(&leaves, index).unwrap();
                assert!(
                    verify(&leaves[index], index, size, &path, &root),
                    "size {size} index {index}"
                );
            }
        }
    }

    #[test]
    fn tampered_leaf_fails_verification() {
        let leaves: Vec<[u8; 32]> = (0..6).map(|i| leaf(&format!("row-{i}"))).collect();
        let root = merkle_root(&leaves).unwrap();
        let path = prove(&leaves, 3).unwrap();
        let mut forged = leaves[3];
        forged[0] ^= 0x01; // flip one row byte
        assert!(!verify(&forged, 3, 6, &path, &root));
    }

    #[test]
    fn tampered_sibling_fails_verification() {
        let leaves: Vec<[u8; 32]> = (0..6).map(|i| leaf(&format!("row-{i}"))).collect();
        let root = merkle_root(&leaves).unwrap();
        let mut path = prove(&leaves, 2).unwrap();
        path[0].hash[31] ^= 0x80;
        assert!(!verify(&leaves[2], 2, 6, &path, &root));
    }

    #[test]
    fn proof_rejects_wrong_position_and_root() {
        let leaves: Vec<[u8; 32]> = (0..5).map(|i| leaf(&format!("row-{i}"))).collect();
        let root = merkle_root(&leaves).unwrap();
        let path = prove(&leaves, 1).unwrap();
        // Replayed at a different index.
        assert!(!verify(&leaves[1], 2, 5, &path, &root));
        // Correct path, wrong root.
        let mut bad_root = root;
        bad_root[0] ^= 0x01;
        assert!(!verify(&leaves[1], 1, 5, &path, &bad_root));
        // Truncated path.
        assert!(!verify(&leaves[1], 1, 5, &path[..1], &root));
    }
}
