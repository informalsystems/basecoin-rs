use super::{
    codec::{BinCodec, Codec, JsonCodec, NullCodec, ProtobufCodec},
    context::Store,
    RevertibleStore, SharedStore,
};
use crate::helper::{Height, Path, RawHeight};
use crate::store::avl::AvlTree;
use std::sync::{Arc, RwLock};
use std::{fmt::Debug, marker::PhantomData};

// A state type that represents a snapshot of the store at every block.
// The value is a `Vec<u8>` to allow stored types to choose their own serde.
pub(crate) type State = AvlTree<Path, Vec<u8>>;

pub(crate) type MainStore<S> = SharedStore<RevertibleStore<S>>;
pub(crate) type SharedRw<T> = Arc<RwLock<T>>;

/// A `TypedStore` that uses the `JsonCodec`
pub(crate) type JsonStore<S, K, V> = TypedStore<S, K, JsonCodec<V>>;

/// A `TypedStore` that uses the `ProtobufCodec`
pub(crate) type ProtobufStore<S, K, V, R> = TypedStore<S, K, ProtobufCodec<V, R>>;

/// A `TypedSet` that stores only paths and no values
pub(crate) type TypedSet<S, K> = TypedStore<S, K, NullCodec>;

/// A `TypedStore` that uses the `BinCodec`
pub(crate) type BinStore<S, K, V> = TypedStore<S, K, BinCodec<V>>;

#[derive(Clone, Debug)]
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
    pub(crate) fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.store.get_keys(key_prefix)
    }

    #[inline]
    pub(crate) fn current_height(&self) -> RawHeight {
        self.store.current_height()
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
