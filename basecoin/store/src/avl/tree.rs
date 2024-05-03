use core::borrow::Borrow;
use core::cmp::Ordering;

use ics23::commitment_proof::Proof;
use ics23::{CommitmentProof, ExistenceProof, HashOp, InnerOp, NonExistenceProof};
use tendermint::hash::Hash;

use super::proof::{get_leaf_op, EMPTY_CHILD};
use super::AvlNode;
use crate::avl::node::{as_node_ref, NodeRef};
use crate::avl::AsBytes;

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

    /// Return the hash of the merkle tree root, if it has at least one node.
    pub fn root_hash(&self) -> Option<&Hash> {
        Some(&self.root.as_ref()?.merkle_hash)
    }

    /// Return the value corresponding to the key, if it exists.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
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
        AvlTree::insert_rec(node_ref, key, value)
    }

    /// Insert a value in the tree.
    fn insert_rec(node_ref: &mut NodeRef<K, V>, key: K, value: V) -> Option<V> {
        if let Some(node) = node_ref {
            let old_value = match node.key.cmp(&key) {
                Ordering::Greater => AvlTree::insert_rec(&mut node.left, key, value),
                Ordering::Less => AvlTree::insert_rec(&mut node.right, key, value),
                Ordering::Equal => Some(node.set_value(value)),
            };
            node.update();
            AvlTree::balance_node(node_ref);
            old_value
        } else {
            *node_ref = as_node_ref(key, value);
            None
        }
    }

    /// Remove a value from the AVL tree, this operation runs in amortized O(log(n)).
    pub fn remove(&mut self, key: K) -> Option<V> {
        let node_ref = &mut self.root;
        AvlTree::remove_rec(node_ref, key)
    }

    /// Remove a value from the tree.
    fn remove_rec(node_ref: &mut NodeRef<K, V>, key: K) -> Option<V> {
        let node = node_ref.as_deref_mut()?;

        let removed_value = match node.key.cmp(&key) {
            Ordering::Greater => AvlTree::remove_rec(&mut node.left, key),
            Ordering::Less => AvlTree::remove_rec(&mut node.right, key),
            Ordering::Equal => AvlTree::remove_root(node_ref).map(|node| node.value),
        };

        if let Some(node) = node_ref {
            node.update();
            AvlTree::balance_node(node_ref);
        }

        removed_value
    }

    /// Removes the root node in the tree, if it exists.
    ///
    /// Since we are removing the root node, we need to replace it with a new node.
    /// The new node is chosen as follows:
    /// - If the root node has no children, the new node is `None`.
    /// - If the root node has only one child, the new node is the child.
    /// - If the root node has both children.
    ///   - If left child is shorter: the new node is the leftmost node in the right subtree.
    ///     Also, the root node's children are set to the new node's children.
    ///   - If right child is shorter, vice versa.
    fn remove_root(node_ref: &mut NodeRef<K, V>) -> NodeRef<K, V> {
        let node = node_ref.as_deref_mut()?;

        let substitute_node_ref = if node.right.is_none() {
            // there is no right node, replace the root node with the left node.
            // substitute_node_ref <- node.left
            node.left.take()
        } else if node.left.is_none() {
            // there is no left node, replace the root node with the right node.
            // substitute_node_ref <- node.right
            node.right.take()
        } else if node.balance_factor() <= 0 {
            // both left and right nodes exist.
            // left.height <= right.height; right skewed.
            // removing from right subtree is better.

            // Remove the leftmost node in the right subtree and replace the current.
            let mut leftmost_node_ref = AvlTree::remove_leftmost(&mut node.right);
            // leftmost_node_ref.right <- node_ref.right
            // leftmost_node_ref.left <- node_ref.left
            if let Some(leftmost_node) = leftmost_node_ref.as_mut() {
                // removed leftmost node must be a leaf; it is an invariant.
                // assert!(leftmost_node.right.is_none() && leftmost_node.left.is_none());

                leftmost_node.right = node.right.take();
                leftmost_node.left = node.left.take();
            }
            // substitute_node_ref <- leftmost_node_ref
            leftmost_node_ref
        } else {
            // node.balance_factor() > 0
            // both left and right nodes exist.
            // left.height > right.height; left skewed.
            // removing from left subtree is better.

            // Remove the rightmost node in the left subtree and replace the current.
            let mut rightmost_node_ref = AvlTree::remove_rightmost(&mut node.left);
            // rightmost_node_ref.right <- node_ref.right
            // rightmost_node_ref.left <- node_ref.left
            if let Some(rightmost_node) = rightmost_node_ref.as_mut() {
                // removed rightmost node must be a leaf; it is an invariant.
                // assert!(rightmost_node.right.is_none() && rightmost_node.left.is_none());

                rightmost_node.right = node.right.take();
                rightmost_node.left = node.left.take();
            }
            // substitute_node_ref <- rightmost_node_ref
            rightmost_node_ref
        };

        // removed_node <- node_ref <- substitute_node_ref
        let removed_node = std::mem::replace(node_ref, substitute_node_ref);

        if let Some(node) = node_ref {
            // need to update, as top node is replaced
            node.update();
            AvlTree::balance_node(node_ref);
        }

        removed_node
    }

    /// Removes the leftmost key in the tree, if it exists.
    fn remove_leftmost(node_ref: &mut NodeRef<K, V>) -> NodeRef<K, V> {
        let node = node_ref.as_deref_mut()?;

        if node.left.is_none() {
            let right_node = node.right.take();
            // removed_node <- node_ref <- right_node
            std::mem::replace(node_ref, right_node)

            // no need to update, as the new node (right_node) is already updated
        } else {
            let removed_node = AvlTree::remove_leftmost(&mut node.left);

            // need to update, as left node is updated
            node.update();
            AvlTree::balance_node(node_ref);

            removed_node
        }
    }

    /// Removes the rightmost key in the tree, if it exists.
    fn remove_rightmost(node_ref: &mut NodeRef<K, V>) -> NodeRef<K, V> {
        let node = node_ref.as_deref_mut()?;

        if node.right.is_none() {
            let left_node = node.left.take();
            // removed_node <- node_ref <- left_node
            std::mem::replace(node_ref, left_node)

            // no need to update, as the new node (left_node) is already updated
        } else {
            let removed_node = AvlTree::remove_rightmost(&mut node.right);

            // need to update, as right node is updated
            node.update();
            AvlTree::balance_node(node_ref);

            removed_node
        }
    }

    /// Return an existence proof for the given element, if it exists.
    /// Otherwise return a non-existence proof.
    pub fn get_proof<Q>(&self, key: &Q) -> CommitmentProof
    where
        K: Borrow<Q>,
        Q: Ord + AsBytes + ?Sized,
    {
        let proof = Self::get_proof_rec(key, &self.root);
        CommitmentProof { proof: Some(proof) }
    }

    fn get_local_existence_proof(node: &AvlNode<K, V>) -> ExistenceProof {
        ExistenceProof {
            key: node.key.as_bytes().as_ref().to_owned(),
            value: node.value.borrow().to_owned(),
            leaf: Some(get_leaf_op()),
            path: vec![InnerOp {
                hash: HashOp::Sha256.into(),
                prefix: node.left_hash().unwrap_or(&EMPTY_CHILD).to_vec(),
                suffix: node.right_hash().unwrap_or(&EMPTY_CHILD).to_vec(),
            }],
        }
    }

    /// Recursively build a proof of existence or non-existence for the desired value.
    fn get_proof_rec<Q>(key: &Q, node: &NodeRef<K, V>) -> Proof
    where
        K: Borrow<Q>,
        Q: Ord + AsBytes + ?Sized,
    {
        if let Some(node) = node {
            match node.key.borrow().cmp(key) {
                Ordering::Greater => {
                    let prefix = vec![];
                    let mut suffix = Vec::with_capacity(64);
                    suffix.extend(node.hash.as_bytes());
                    suffix.extend(node.right_hash().unwrap_or(&EMPTY_CHILD));
                    let inner = InnerOp {
                        hash: HashOp::Sha256.into(),
                        prefix,
                        suffix,
                    };
                    match Self::get_proof_rec(key, &node.left) {
                        Proof::Exist(mut proof) => {
                            proof.path.push(inner);
                            Proof::Exist(proof)
                        }
                        Proof::Nonexist(mut proof) => {
                            if let Some(right) = proof.right.as_mut() {
                                // right-neighbor already found
                                right.path.push(inner.clone());
                            }
                            if let Some(left) = proof.left.as_mut() {
                                // left-neighbor already found
                                left.path.push(inner);
                            }
                            if proof.right.is_none() {
                                // found the right-neighbor
                                proof.right = Some(Self::get_local_existence_proof(node));
                            }
                            Proof::Nonexist(proof)
                        }
                        _ => unreachable!(),
                    }
                }
                Ordering::Less => {
                    let suffix = vec![];
                    let mut prefix = Vec::with_capacity(64);
                    prefix.extend(node.left_hash().unwrap_or(&EMPTY_CHILD));
                    prefix.extend(node.hash.as_bytes());
                    let inner = InnerOp {
                        hash: HashOp::Sha256.into(),
                        prefix,
                        suffix,
                    };
                    match Self::get_proof_rec(key, &node.right) {
                        Proof::Exist(mut proof) => {
                            proof.path.push(inner);
                            Proof::Exist(proof)
                        }
                        Proof::Nonexist(mut proof) => {
                            if let Some(right) = proof.right.as_mut() {
                                // right-neighbor already found
                                right.path.push(inner.clone());
                            }
                            if let Some(left) = proof.left.as_mut() {
                                // left-neighbor already found
                                left.path.push(inner);
                            }
                            if proof.left.is_none() {
                                // found the left-neighbor
                                proof.left = Some(Self::get_local_existence_proof(node));
                            }
                            Proof::Nonexist(proof)
                        }
                        _ => unreachable!(),
                    }
                }
                Ordering::Equal => Proof::Exist(Self::get_local_existence_proof(node)),
            }
        } else {
            Proof::Nonexist(NonExistenceProof {
                key: key.as_bytes().as_ref().to_owned(),
                left: None,
                right: None,
            })
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
        core::mem::swap(&mut node.left, &mut left_right);
        node.update();
        core::mem::swap(&mut left.right, &mut Some(node));
        left.update();
        core::mem::swap(root, &mut Some(left));
    }

    /// Perform a left rotation.
    pub fn rotate_left(root: &mut NodeRef<K, V>) {
        let mut node = root.take().expect("[AVL]: Empty root in left rotation");
        let mut right = node.right.take().expect("[AVL]: Unexpected left rotation");
        let mut right_left = right.left.take();
        core::mem::swap(&mut node.right, &mut right_left);
        node.update();
        core::mem::swap(&mut right.left, &mut Some(node));
        right.update();
        core::mem::swap(root, &mut Some(right))
    }

    /// Return a list of the keys present in the tree.
    pub fn get_keys(&self) -> Vec<&K> {
        let mut keys = Vec::new();
        Self::get_keys_rec(&self.root, &mut keys);
        keys
    }

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
