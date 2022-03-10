pub(crate) mod codec;

use super::{Height, Path, Store};
use codec::Codec;

use std::marker::PhantomData;

#[derive(Clone)]
pub(crate) struct TypedStore<S, C, K, V> {
    store: S,
    _codec: PhantomData<C>,
    _key: PhantomData<K>,
    _value: PhantomData<V>,
}

impl<S, C, K, V> TypedStore<S, C, K, V>
where
    S: Store,
    for<'a> C: Codec<'a, Type = V>,
    K: Into<Path> + Clone,
{
    #[inline]
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            _codec: PhantomData,
            _key: PhantomData,
            _value: PhantomData,
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
}
