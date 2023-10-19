use core::{
    borrow::Borrow,
    cmp::{Ord, Ordering},
    marker::Sized,
    option::{
        Option,
        Option::{None, Some},
    },
};

use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, LeafOp, LengthOp,
};
use tendermint::hash::Hash;

use crate::avl::{
    node::{as_node_ref, NodeRef},
    proof, AsBytes,
};

/// An AVL Tree that supports `get` and `insert` operation and can be used to prove existence of a
/// given key-value couple.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct AvlTree<K: Ord + AsBytes, V> {
    pub root: NodeRef<K, V>,
}

impl<K: Ord + AsBytes, V: Borrow<[u8]>> AvlTree<K, V> {
    /// Return an empty AVL tree.
    pub fn new() -> Self {
        AvlTree { root: None }
    }

    #[allow(dead_code)]
    /// Return the hash of the merkle tree root, if it has at least one node.
    pub fn root_hash(&self) -> Option<&Hash> {
        Some(&self.root.as_ref()?.merkle_hash)
    }

    /// Return the value corresponding to the key, if it exists.
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut node_ref = &self.root;
        while let Some(ref node) = node_ref {
            match node.key.borrow().cmp(key) {
                Ordering::Greater => node_ref = &node.left,
                Ordering::Less => node_ref = &node.right,
                Ordering::Equal => return Some(&node.value),
            }
        }
        None
    }

    /// Insert a value into the AVL tree, this operation runs in amortized O(log(n)).
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let node_ref = &mut self.root;
        let mut old_value = None;
        AvlTree::insert_rec(node_ref, key, value, &mut old_value);
        old_value
    }

    /// Insert a value in the tree.
    fn insert_rec(node_ref: &mut NodeRef<K, V>, key: K, value: V, old_value: &mut Option<V>) {
        if let Some(node) = node_ref {
            match node.key.cmp(&key) {
                Ordering::Greater => AvlTree::insert_rec(&mut node.left, key, value, old_value),
                Ordering::Less => AvlTree::insert_rec(&mut node.right, key, value, old_value),
                Ordering::Equal => *old_value = Some(node.set_value(value)),
            }
            node.update();
            AvlTree::balance_node(node_ref);
        } else {
            *node_ref = as_node_ref(key, value);
        }
    }

    /// Remove a value from the AVL tree, this operation runs in amortized O(log(n)).
    pub fn remove(&mut self, key: K) -> Option<V> {
        let node_ref = &mut self.root;
        let mut old_value = None;
        AvlTree::remove_rec(node_ref, key, &mut old_value);
        old_value
    }

    /// Remove a value from the tree.
    fn remove_rec(node_ref: &mut NodeRef<K, V>, key: K, old_value: &mut Option<V>) {
        if let Some(node) = node_ref {
            match node.key.cmp(&key) {
                Ordering::Greater => AvlTree::remove_rec(&mut node.left, key, old_value),
                Ordering::Less => AvlTree::remove_rec(&mut node.right, key, old_value),
                Ordering::Equal => {
                    let mut removed_node = None;
                    AvlTree::remove_top(node_ref, &mut removed_node);
                    *old_value = removed_node.map(|node| node.value);
                }
            }
        }

        if let Some(node) = node_ref {
            node.update();
            AvlTree::balance_node(node_ref);
        }
    }

    /// Removes the top node in the tree, if it exists.
    fn remove_top(node_ref: &mut NodeRef<K, V>, removed_node: &mut NodeRef<K, V>) {
        if let Some(node) = node_ref {
            if node.right.is_some() {
                // Remove the leftmost node in the right subtree and replace the current.
                let mut leftmost_node_ref = None;
                AvlTree::remove_leftmost(&mut node.right, &mut leftmost_node_ref);
                // leftmost_node_ref.right <- node_ref.right
                // leftmost_node_ref.left <- node_ref.left
                // removed_node <- node_ref <- leftmost_node_ref
                if let Some(leftmost_node) = leftmost_node_ref.as_mut() {
                    assert!(
                        std::mem::replace(&mut leftmost_node.right, node.right.take()).is_none()
                    );
                    assert!(std::mem::replace(&mut leftmost_node.left, node.left.take()).is_none());
                }
                assert!(std::mem::replace(
                    removed_node,
                    std::mem::replace(node_ref, leftmost_node_ref)
                )
                .is_none());
            } else if node.left.is_some() {
                // Remove the rightmost node in the left subtree and replace the current.
                let mut rightmost_node_ref = None;
                AvlTree::remove_rightmost(&mut node.left, &mut rightmost_node_ref);
                // rightmost_node_ref.right <- node_ref.right
                // rightmost_node_ref.left <- node_ref.left
                // removed_node <- node_ref <- rightmost_node
                if let Some(rightmost_node) = rightmost_node_ref.as_mut() {
                    assert!(
                        std::mem::replace(&mut rightmost_node.right, node.right.take()).is_none()
                    );
                    assert!(
                        std::mem::replace(&mut rightmost_node.left, node.left.take()).is_none()
                    );
                }
                assert!(std::mem::replace(
                    removed_node,
                    std::mem::replace(node_ref, rightmost_node_ref)
                )
                .is_none());
            } else {
                // The node is a leaf, remove it.
                assert!(std::mem::replace(removed_node, node_ref.take()).is_none());
            }
        }

        if let Some(node) = node_ref {
            node.update();
            AvlTree::balance_node(node_ref);
        }
    }

    /// Removes the leftmost key in the tree, if it exists.
    fn remove_leftmost(node_ref: &mut NodeRef<K, V>, removed_node: &mut NodeRef<K, V>) {
        if let Some(node) = node_ref {
            if node.left.is_none() {
                let right_node = node.right.take();
                // removed_node <- node_ref <- right_node
                assert!(
                    std::mem::replace(removed_node, std::mem::replace(node_ref, right_node))
                        .is_none()
                );
            } else {
                AvlTree::remove_leftmost(&mut node.left, removed_node);
            }
        }

        if let Some(node) = node_ref {
            node.update();
            AvlTree::balance_node(node_ref);
        }
    }

    /// Removes the rightmost key in the tree, if it exists.
    fn remove_rightmost(node_ref: &mut NodeRef<K, V>, removed_node: &mut NodeRef<K, V>) {
        if let Some(node) = node_ref {
            if node.right.is_none() {
                let left_node = node.left.take();
                // removed_node <- node_ref <- left_node
                assert!(
                    std::mem::replace(removed_node, std::mem::replace(node_ref, left_node))
                        .is_none()
                );
            } else {
                AvlTree::remove_rightmost(&mut node.right, removed_node);
            }
        }

        if let Some(node) = node_ref {
            node.update();
            AvlTree::balance_node(node_ref);
        }
    }

    #[allow(dead_code)]
    /// Return an existence proof for the given element, if it exists.
    pub fn get_proof<Q: ?Sized>(&self, key: &Q) -> Option<CommitmentProof>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let proof = Self::get_proof_rec(key, &self.root)?;
        Some(CommitmentProof {
            proof: Some(Proof::Exist(proof)),
        })
    }

    /// Recursively build a proof of existence for the desired value.
    fn get_proof_rec<Q: ?Sized>(key: &Q, node: &NodeRef<K, V>) -> Option<ExistenceProof>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        if let Some(node) = node {
            let empty_hash = [];
            let (mut proof, prefix, suffix) = match node.key.borrow().cmp(key) {
                Ordering::Greater => {
                    let proof = Self::get_proof_rec(key, &node.left)?;
                    let prefix = vec![];
                    let mut suffix = Vec::with_capacity(64);
                    suffix.extend(node.hash.as_bytes());
                    suffix.extend(node.right_hash().unwrap_or(&empty_hash));
                    (proof, prefix, suffix)
                }
                Ordering::Less => {
                    let proof = Self::get_proof_rec(key, &node.right)?;
                    let suffix = vec![];
                    let mut prefix = Vec::with_capacity(64);
                    prefix.extend(node.left_hash().unwrap_or(&empty_hash));
                    prefix.extend(node.hash.as_bytes());
                    (proof, prefix, suffix)
                }
                Ordering::Equal => {
                    let leaf = Some(LeafOp {
                        hash: HashOp::Sha256.into(),
                        prehash_key: HashOp::NoHash.into(),
                        prehash_value: HashOp::NoHash.into(),
                        length: LengthOp::NoPrefix.into(),
                        prefix: proof::LEAF_PREFIX.to_vec(),
                    });
                    let proof = ExistenceProof {
                        key: node.key.as_bytes().as_ref().to_owned(),
                        value: node.value.borrow().to_owned(),
                        leaf,
                        path: vec![],
                    };
                    let prefix = node.left_hash().unwrap_or(&empty_hash).to_vec();
                    let suffix = node.right_hash().unwrap_or(&empty_hash).to_vec();
                    (proof, prefix, suffix)
                }
            };
            let inner = InnerOp {
                hash: HashOp::Sha256.into(),
                prefix,
                suffix,
            };
            proof.path.push(inner);
            Some(proof)
        } else {
            None
        }
    }

    /// Rebalance the AVL tree by performing rotations, if needed.
    fn balance_node(node_ref: &mut NodeRef<K, V>) {
        let node = node_ref
            .as_mut()
            .expect("[AVL]: Empty node in node balance");
        let balance_factor = node.balance_factor();
        if balance_factor >= 2 {
            let left = node
                .left
                .as_mut()
                .expect("[AVL]: Unexpected empty left node");
            if left.balance_factor() < 1 {
                AvlTree::rotate_left(&mut node.left);
            }
            AvlTree::rotate_right(node_ref);
        } else if balance_factor <= -2 {
            let right = node
                .right
                .as_mut()
                .expect("[AVL]: Unexpected empty right node");
            if right.balance_factor() > -1 {
                AvlTree::rotate_right(&mut node.right);
            }
            AvlTree::rotate_left(node_ref);
        }
    }

    /// Performs a right rotation.
    pub fn rotate_right(root: &mut NodeRef<K, V>) {
        let mut node = root.take().expect("[AVL]: Empty root in right rotation");
        let mut left = node.left.take().expect("[AVL]: Unexpected right rotation");
        let mut left_right = left.right.take();
        std::mem::swap(&mut node.left, &mut left_right);
        node.update();
        std::mem::swap(&mut left.right, &mut Some(node));
        left.update();
        std::mem::swap(root, &mut Some(left));
    }

    /// Perform a left rotation.
    pub fn rotate_left(root: &mut NodeRef<K, V>) {
        let mut node = root.take().expect("[AVL]: Empty root in left rotation");
        let mut right = node.right.take().expect("[AVL]: Unexpected left rotation");
        let mut right_left = right.left.take();
        std::mem::swap(&mut node.right, &mut right_left);
        node.update();
        std::mem::swap(&mut right.left, &mut Some(node));
        right.update();
        std::mem::swap(root, &mut Some(right))
    }

    #[allow(dead_code)]
    /// Return a list of the keys present in the tree.
    pub fn get_keys(&self) -> Vec<&K> {
        let mut keys = Vec::new();
        Self::get_keys_rec(&self.root, &mut keys);
        keys
    }

    #[allow(dead_code)]
    fn get_keys_rec<'a>(node_ref: &'a NodeRef<K, V>, keys: &mut Vec<&'a K>) {
        if let Some(node) = node_ref {
            Self::get_keys_rec(&node.left, keys);
            keys.push(&node.key);
            Self::get_keys_rec(&node.right, keys);
        }
    }
}

impl<K: Ord + AsBytes, V: Borrow<[u8]>> Default for AvlTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AvlTree;
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn insert_and_remove() {
        let mut tree = AvlTree::new();

        let mut keys: Vec<u8> = (0..100).collect();

        keys.shuffle(&mut thread_rng());
        for &i in keys.iter() {
            tree.insert([i], vec![i]);
        }

        keys.shuffle(&mut thread_rng());
        for &i in keys.iter() {
            assert_eq!(tree.remove([i]), Some(vec![i]));
        }

        assert_eq!(tree.root, None);
    }
}
