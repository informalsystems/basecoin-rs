mod avl;
mod memory;

pub(crate) use memory::InMemoryStore;

use crate::app::modules::Error as ModuleError;

use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::str::{from_utf8, Utf8Error};
use std::sync::{Arc, RwLock};

use flex_error::{define_error, TraceError};
use ibc::core::ics24_host::{error::ValidationError, validate::validate_identifier};
use ics23::CommitmentProof;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use tracing::trace;

/// A `TypedStore` that uses the `JsonCodec`
pub(crate) type JsonStore<S, K, V> = TypedStore<S, K, JsonCodec<V>>;

/// A `TypedStore` that uses the `ProtobufCodec`
pub(crate) type ProtobufStore<S, K, V, R> = TypedStore<S, K, ProtobufCodec<V, R>>;

/// A `TypedSet` that stores only paths and no values
pub(crate) type TypedSet<S, K> = TypedStore<S, K, NullCodec>;

/// A newtype representing a valid ICS024 identifier.
/// Implements `Deref<Target=String>`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Identifier(String);

impl Identifier {
    /// Identifiers MUST be non-empty (of positive integer length).
    /// Identifiers MUST consist of characters in one of the following categories only:
    /// * Alphanumeric
    /// * `.`, `_`, `+`, `-`, `#`
    /// * `[`, `]`, `<`, `>`
    fn validate(s: impl AsRef<str>) -> Result<(), Error> {
        let s = s.as_ref();

        // give a `min` parameter of 0 here to allow id's of arbitrary
        // length as inputs; `validate_identifier` itself checks for
        // empty inputs and returns an error as appropriate
        validate_identifier(s, 0, s.len()).map_err(|v| Error::invalid_identifier(s.to_string(), v))
    }
}

impl Deref for Identifier {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Identifier {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Identifier::validate(&s).map(|_| Self(s))
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A newtype representing a valid ICS024 `Path`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]

pub struct Path(Vec<Identifier>);

impl TryFrom<String> for Path {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let mut identifiers = vec![];
        let parts = s.split('/'); // split will never return an empty iterator
        for part in parts {
            identifiers.push(Identifier::try_from(part.to_owned())?);
        }
        Ok(Self(identifiers))
    }
}

impl TryFrom<&[u8]> for Path {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let s = from_utf8(value).map_err(Error::malformed_path_string)?;
        s.to_owned().try_into()
    }
}

impl From<Identifier> for Path {
    fn from(id: Identifier) -> Self {
        Self(vec![id])
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|iden| iden.as_str().to_owned())
                .collect::<Vec<String>>()
                .join("/")
        )
    }
}

define_error! {
    #[derive(Eq, PartialEq)]
    Error {
        InvalidIdentifier
            { identifier: String }
            [ ValidationError ]
            | e | { format!("'{}' is not a valid identifier", e.identifier) },
        MalformedPathString
            [ TraceError<Utf8Error> ]
            | _ | { "path isn't a valid string" },

    }
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::store(e)
    }
}

/// Block height
pub(crate) type RawHeight = u64;

/// Store height to query
#[derive(Debug, Copy, Clone)]
pub enum Height {
    Pending,
    Latest,
    Stable(RawHeight), // or equivalently `tendermint::block::Height`
}

impl From<RawHeight> for Height {
    fn from(value: u64) -> Self {
        match value {
            0 => Height::Latest, // see https://docs.tendermint.com/master/spec/abci/abci.html#query
            _ => Height::Stable(value),
        }
    }
}

/// Store trait - maybe provableStore or privateStore
pub trait Store: Send + Sync + Clone {
    /// Error type - expected to envelope all possible errors in store
    type Error: core::fmt::Debug;

    /// Set `value` for `path`
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Get associated `value` for `path` at specified `height`
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>>;

    /// Delete specified `path`
    fn delete(&mut self, path: &Path);

    /// Commit `Pending` block to canonical chain and create new `Pending`
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error>;

    /// Apply accumulated changes to `Pending`
    fn apply(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Reset accumulated changes
    fn reset(&mut self) {}

    /// Prune historic blocks upto specified `height`
    fn prune(&mut self, height: RawHeight) -> Result<RawHeight, Self::Error> {
        Ok(height)
    }

    /// Return the current height of the chain
    fn current_height(&self) -> RawHeight;

    /// Return all keys that start with specified prefix
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path>; // TODO(hu55a1n1): implement support for all heights
}

/// ProvableStore trait
pub trait ProvableStore: Store {
    /// Return a vector commitment
    fn root_hash(&self) -> Vec<u8>;

    /// Return proof of existence for key
    fn get_proof(&self, height: Height, key: &Path) -> Option<ics23::CommitmentProof>;
}

/// Wraps a store to make it shareable by cloning
#[derive(Clone)]
pub struct SharedStore<S>(Arc<RwLock<S>>);

impl<S> SharedStore<S> {
    pub(crate) fn new(store: S) -> Self {
        Self(Arc::new(RwLock::new(store)))
    }

    pub(crate) fn share(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> Default for SharedStore<S>
where
    S: Default + Store,
{
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S> Store for SharedStore<S>
where
    S: Store,
{
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
        self.write().unwrap().set(path, value)
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.read().unwrap().get(height, path)
    }

    #[inline]
    fn delete(&mut self, path: &Path) {
        self.write().unwrap().delete(path)
    }

    #[inline]
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        self.write().unwrap().commit()
    }

    #[inline]
    fn apply(&mut self) -> Result<(), Self::Error> {
        self.write().unwrap().apply()
    }

    #[inline]
    fn reset(&mut self) {
        self.write().unwrap().reset()
    }

    #[inline]
    fn current_height(&self) -> RawHeight {
        self.read().unwrap().current_height()
    }

    #[inline]
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.read().unwrap().get_keys(key_prefix)
    }
}

impl<S> ProvableStore for SharedStore<S>
where
    S: ProvableStore,
{
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.read().unwrap().root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.read().unwrap().get_proof(height, key)
    }
}

impl<S> Deref for SharedStore<S> {
    type Target = Arc<RwLock<S>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> DerefMut for SharedStore<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// A wrapper store that implements rudimentary `apply()`/`reset()` support for other stores
#[derive(Clone)]
pub(crate) struct RevertibleStore<S> {
    /// backing store
    store: S,
    /// operation log for recording rollback operations in preserved order
    op_log: Vec<RevertOp>,
}

#[derive(Clone)]
enum RevertOp {
    Delete(Path),
    Set(Path, Vec<u8>),
}

impl<S> RevertibleStore<S>
where
    S: Store,
{
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            op_log: vec![],
        }
    }
}

impl<S> Default for RevertibleStore<S>
where
    S: Default + Store,
{
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S> Store for RevertibleStore<S>
where
    S: Store,
{
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
        let old_value = self.store.set(path.clone(), value)?;
        match old_value {
            // None implies this was an insert op, so we record the revert op as delete op
            None => self.op_log.push(RevertOp::Delete(path)),
            // Some old value implies this was an update op, so we record the revert op as a set op
            // with the old value
            Some(ref old_value) => self.op_log.push(RevertOp::Set(path, old_value.clone())),
        }
        Ok(old_value)
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.store.get(height, path)
    }

    #[inline]
    fn delete(&mut self, _path: &Path) {
        unimplemented!("RevertibleStore doesn't support delete operations yet!")
    }

    #[inline]
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        // call `apply()` before `commit()` to make sure all operations are applied
        self.apply()?;
        self.store.commit()
    }

    #[inline]
    fn apply(&mut self) -> Result<(), Self::Error> {
        // note that we do NOT call the backing store's apply here - this allows users to create
        // multilayered `WalStore`s
        self.op_log.clear();
        Ok(())
    }

    #[inline]
    fn reset(&mut self) {
        // note that we do NOT call the backing store's reset here - this allows users to create
        // multilayered `WalStore`s
        trace!("Rollback operation log changes");
        while let Some(op) = self.op_log.pop() {
            match op {
                RevertOp::Delete(path) => self.delete(&path),
                RevertOp::Set(path, value) => {
                    self.set(path, value).unwrap(); // safety - reset failures are unrecoverable
                }
            }
        }
    }

    #[inline]
    fn current_height(&self) -> u64 {
        self.store.current_height()
    }

    #[inline]
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.store.get_keys(key_prefix)
    }
}

impl<S> ProvableStore for RevertibleStore<S>
where
    S: ProvableStore,
{
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.store.root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.store.get_proof(height, key)
    }
}

/// A trait that defines how types are decoded/encoded.
pub(crate) trait Codec {
    type Type;
    type Encoded: AsRef<[u8]>;

    fn encode(d: &Self::Type) -> Option<Self::Encoded>;

    fn decode(bytes: &[u8]) -> Option<Self::Type>;
}

/// A JSON codec that uses `serde_json` to encode/decode as a JSON string
#[derive(Clone)]
pub(crate) struct JsonCodec<T>(PhantomData<T>);

impl<T> Codec for JsonCodec<T>
where
    T: Serialize + DeserializeOwned,
{
    type Type = T;
    type Encoded = String;

    fn encode(d: &Self::Type) -> Option<Self::Encoded> {
        serde_json::to_string(d).ok()
    }

    fn decode(bytes: &[u8]) -> Option<Self::Type> {
        let json_string = String::from_utf8(bytes.to_vec()).ok()?;
        serde_json::from_str(&json_string).ok()
    }
}

/// A Null codec that can be used for paths that are only meant to be set/reset and do not hold any
/// typed value.
#[derive(Clone)]
pub(crate) struct NullCodec;

impl Codec for NullCodec {
    type Type = ();
    type Encoded = Vec<u8>;

    fn encode(_d: &Self::Type) -> Option<Self::Encoded> {
        Some(vec![])
    }

    fn decode(bytes: &[u8]) -> Option<Self::Type> {
        assert!(bytes.is_empty());
        Some(())
    }
}

/// A Protobuf codec that uses `prost` to encode/decode
#[derive(Clone)]
pub(crate) struct ProtobufCodec<T, R> {
    domain_type: PhantomData<T>,
    raw_type: PhantomData<R>,
}

impl<T, R> Codec for ProtobufCodec<T, R>
where
    T: Into<R> + Clone,
    R: TryInto<T> + Default + prost::Message,
{
    type Type = T;
    type Encoded = Vec<u8>;

    fn encode(d: &Self::Type) -> Option<Self::Encoded> {
        let r = d.clone().into();
        Some(r.encode_to_vec())
    }

    fn decode(bytes: &[u8]) -> Option<Self::Type> {
        let r = R::decode(bytes).ok()?;
        r.try_into().ok()
    }
}

/// The `TypedStore` provides methods to treat the data stored at given store paths as given Rust types.
///
/// It is designed to be aliased for each concrete codec. For example,
/// ```rust
/// type CandyStore<S, K, V> = TypedStore<S, K, CandyCodec<V>>;
/// ```
#[derive(Clone)]
pub(crate) struct TypedStore<S, K, C> {
    store: S,
    _key: PhantomData<K>,
    _codec: PhantomData<C>,
}

impl<S, K, C, V> TypedStore<S, K, C>
where
    S: Store,
    C: Codec<Type = V>,
    K: Into<Path> + Clone,
{
    #[inline]
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            _codec: PhantomData,
            _key: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn set(&mut self, path: K, value: V) -> Result<Option<V>, S::Error> {
        self.store
            .set(path.into(), C::encode(&value).unwrap().as_ref().to_vec())
            .map(|prev_val| prev_val.and_then(|v| C::decode(&v)))
    }

    #[inline]
    pub(crate) fn get(&self, height: Height, path: &K) -> Option<V> {
        self.store
            .get(height, &path.clone().into())
            .and_then(|v| C::decode(&v))
    }

    #[inline]
    pub(crate) fn delete(&mut self, path: &K) {
        self.store.delete(&path.clone().into())
    }
}

impl<S, K> TypedStore<S, K, NullCodec>
where
    S: Store,
    K: Into<Path> + Clone,
{
    #[inline]
    pub(crate) fn set_path(&mut self, path: K) -> Result<(), S::Error> {
        self.store.set(path.into(), vec![]).map(|_| ())
    }

    #[inline]
    pub(crate) fn is_path_set(&self, height: Height, path: &K) -> bool {
        self.store.get(height, &path.clone().into()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{Identifier, Path};

    use lazy_static::lazy_static;
    use proptest::prelude::*;
    use rand::distributions::Standard;
    use rand::seq::SliceRandom;
    use std::collections::HashSet;
    use std::convert::TryFrom;

    const ALLOWED_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                   abcdefghijklmnopqrstuvwxyz\
                                   ._+-#[]<>";

    lazy_static! {
        static ref VALID_CHARS: HashSet<char> = {
            ALLOWED_CHARS
                .iter()
                .map(|c| char::from(*c))
                .collect::<HashSet<_>>()
        };
    }

    fn gen_valid_identifier(len: usize) -> String {
        let mut rng = rand::thread_rng();

        (0..=len)
            .map(|_| {
                let idx = rng.gen_range(0..ALLOWED_CHARS.len());
                ALLOWED_CHARS[idx] as char
            })
            .collect::<String>()
    }

    fn gen_invalid_identifier(len: usize) -> String {
        let mut rng = rand::thread_rng();

        (0..=len)
            .map(|_| loop {
                let c = rng.sample::<char, _>(Standard) as char;

                if c.is_ascii() && !VALID_CHARS.contains(&c) {
                    return c;
                }
            })
            .collect::<String>()
    }

    proptest! {
        #[test]
        fn validate_method_doesnt_crash(s in "\\PC*") {
            let _ = Identifier::validate(&s);
        }

        #[test]
        fn valid_identifier_is_ok(l in 1usize..=10) {
            let id = gen_valid_identifier(l);
            let validated = Identifier::validate(&id);

            assert!(validated.is_ok())
        }

        #[test]
        #[ignore]
        fn invalid_identifier_errors(l in 1usize..=10) {
            let id = gen_invalid_identifier(l);
            let validated = Identifier::validate(&id);

            assert!(validated.is_err())
        }

        #[test]
        fn path_with_valid_parts_is_valid(n_parts in 1usize..=10) {
            let mut rng = rand::thread_rng();

            let parts = (0..n_parts)
                .map(|_| {
                    let len = rng.gen_range(1usize..=10);
                    gen_valid_identifier(len)
                })
                .collect::<Vec<_>>();

            let path = parts.join("/");

            assert!(Path::try_from(path).is_ok());
        }

        #[test]
        #[ignore]
        fn path_with_invalid_parts_is_invalid(n_parts in 1usize..=10) {
            let mut rng = rand::thread_rng();
            let n_invalid_parts = rng.gen_range(1usize..=n_parts);
            let n_valid_parts = n_parts - n_invalid_parts;

            let mut parts = (0..n_invalid_parts)
                .map(|_| {
                    let len = rng.gen_range(1usize..=10);
                    gen_invalid_identifier(len)
                })
                .collect::<Vec<_>>();

            let mut valid_parts = (0..n_valid_parts)
                .map(|_| {
                    let len = rng.gen_range(1usize..=10);
                    gen_valid_identifier(len)
                })
                .collect::<Vec<_>>();

            parts.append(&mut valid_parts);
            parts.shuffle(&mut rng);

            let path = parts.join("/");

            assert!(Path::try_from(path).is_err());
        }
    }
}
