use super::Error;
use pallas::crypto::hash::Hash;
use rocksdb::TransactionDB;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

// TODO: unwraps ?
// TODO: audit

#[derive(PartialEq, Eq, Debug)]
pub struct DBHash32(pub Hash<32>);

impl From<Box<[u8]>> for DBHash32 {
    fn from(value: Box<[u8]>) -> Self {
        let inner: [u8; 32] = value[0..32].try_into().unwrap();
        let inner = Hash::<32>::from(inner);
        Self(inner)
    }
}

impl From<DBHash32> for Box<[u8]> {
    fn from(value: DBHash32) -> Self {
        let b = value.0.to_vec();
        b.into()
    }
}

impl From<Hash<32>> for DBHash32 {
    fn from(value: Hash<32>) -> Self {
        DBHash32(value)
    }
}

impl From<DBHash32> for Hash<32> {
    fn from(value: DBHash32) -> Self {
        value.0
    }
}

#[derive(PartialEq, Eq)]
pub struct DBHash28(pub Hash<28>);

impl From<Box<[u8]>> for DBHash28 {
    fn from(value: Box<[u8]>) -> Self {
        let inner: [u8; 28] = value[0..28].try_into().unwrap();
        let inner = Hash::<28>::from(inner);
        Self(inner)
    }
}

impl From<DBHash28> for Box<[u8]> {
    fn from(value: DBHash28) -> Self {
        let b = value.0.to_vec();
        b.into()
    }
}

impl From<Hash<28>> for DBHash28 {
    fn from(value: Hash<28>) -> Self {
        DBHash28(value)
    }
}

impl From<DBHash28> for Hash<28> {
    fn from(value: DBHash28) -> Self {
        value.0
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct DBInt(pub u64);

impl From<DBInt> for Box<[u8]> {
    fn from(value: DBInt) -> Self {
        let b = value.0.to_be_bytes();
        Box::new(b)
    }
}

impl From<Box<[u8]>> for DBInt {
    fn from(value: Box<[u8]>) -> Self {
        let inner: [u8; 8] = value[0..8].try_into().unwrap();
        let inner = u64::from_be_bytes(inner);
        Self(inner)
    }
}

impl From<u64> for DBInt {
    fn from(value: u64) -> Self {
        DBInt(value)
    }
}

impl From<DBInt> for u64 {
    fn from(value: DBInt) -> Self {
        value.0
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct DBBytes(pub Vec<u8>);

impl From<DBBytes> for Box<[u8]> {
    fn from(value: DBBytes) -> Self {
        value.0.into()
    }
}

impl From<Box<[u8]>> for DBBytes {
    fn from(value: Box<[u8]>) -> Self {
        Self(value.into())
    }
}

impl<V> From<DBSerde<V>> for DBBytes
where
    V: Serialize,
{
    fn from(value: DBSerde<V>) -> Self {
        let inner = bincode::serialize(&value.0).unwrap();
        DBBytes(inner)
    }
}

#[derive(Debug, PartialEq)]
pub struct DBSerde<V>(pub V);

impl<V> std::ops::Deref for DBSerde<V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> From<DBSerde<V>> for Box<[u8]>
where
    V: Serialize,
{
    fn from(v: DBSerde<V>) -> Self {
        bincode::serialize(&v.0)
            .map(|x| x.into_boxed_slice())
            .unwrap()
    }
}

impl<V> From<Box<[u8]>> for DBSerde<V>
where
    V: DeserializeOwned,
{
    fn from(value: Box<[u8]>) -> Self {
        let inner = bincode::deserialize(&value).unwrap();
        DBSerde(inner)
    }
}

impl<V> From<DBBytes> for DBSerde<V>
where
    V: DeserializeOwned,
{
    fn from(value: DBBytes) -> Self {
        let inner = bincode::deserialize(&value.0).unwrap();
        DBSerde(inner)
    }
}

impl<V> Clone for DBSerde<V>
where
    V: Clone,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> DBSerde<T> {
    pub fn unwrap(self) -> T {
        self.0
    }
}

pub struct WithDBIntPrefix<T>(pub u64, pub T);

impl<T> From<WithDBIntPrefix<T>> for Box<[u8]>
where
    Box<[u8]>: From<T>,
{
    fn from(value: WithDBIntPrefix<T>) -> Self {
        let prefix: Box<[u8]> = DBInt(value.0).into();
        let after: Box<[u8]> = value.1.into();

        [prefix, after].concat().into()
    }
}

impl<T> From<Box<[u8]>> for WithDBIntPrefix<T> {
    fn from(_value: Box<[u8]>) -> Self {
        todo!()
    }
}

type RocksIterator<'a> =
    rocksdb::DBIteratorWithThreadMode<'a, rocksdb::Transaction<'a, rocksdb::TransactionDB>>;

type NoTxRocksIterator<'a> = rocksdb::DBIteratorWithThreadMode<'a, rocksdb::TransactionDB>;

type SnapshotRocksIterator<'a> = rocksdb::DBIteratorWithThreadMode<'a, rocksdb::TransactionDB>;

pub struct ValueIterator<'a, V>(RocksIterator<'a>, PhantomData<V>);

impl<'a, V> ValueIterator<'a, V> {
    pub fn new(inner: RocksIterator<'a>) -> Self {
        Self(inner, Default::default())
    }
}

impl<'a, V> Iterator for ValueIterator<'a, V>
where
    V: From<Box<[u8]>>,
{
    type Item = Result<V, Error>;

    fn next(&mut self) -> Option<Result<V, Error>> {
        match self.0.next() {
            Some(Ok((_, value))) => Some(Ok(V::from(value))),
            Some(Err(err)) => {
                tracing::error!(?err);
                Some(Err(Error::Rocks(err)))
            }
            None => None,
        }
    }
}

pub struct KeyIterator<'a, K>(RocksIterator<'a>, PhantomData<K>);

impl<'a, K> KeyIterator<'a, K> {
    pub fn new(inner: RocksIterator<'a>) -> Self {
        Self(inner, Default::default())
    }
}

impl<'a, K> Iterator for KeyIterator<'a, K>
where
    K: From<Box<[u8]>>,
{
    type Item = Result<K, Error>;

    fn next(&mut self) -> Option<Result<K, Error>> {
        match self.0.next() {
            Some(Ok((key, _))) => Some(Ok(K::from(key))),
            Some(Err(err)) => {
                tracing::error!(?err);
                Some(Err(Error::Rocks(err)))
            }
            None => None,
        }
    }
}

pub struct EntryIterator<'a, K, V>(RocksIterator<'a>, PhantomData<(K, V)>);

impl<'a, K, V> EntryIterator<'a, K, V> {
    pub fn new(inner: RocksIterator<'a>) -> Self {
        Self(inner, Default::default())
    }
}

impl<'a, K, V> Iterator for EntryIterator<'a, K, V>
where
    K: From<Box<[u8]>>,
    V: From<Box<[u8]>>,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Result<(K, V), Error>> {
        match self.0.next() {
            Some(Ok((key, value))) => {
                let key_out = K::from(key);
                let value_out = V::from(value);

                Some(Ok((key_out, value_out)))
            }
            Some(Err(err)) => {
                tracing::error!(?err);
                Some(Err(Error::Rocks(err)))
            }
            None => None,
        }
    }
}

pub struct NoTxEntryIterator<'a, K, V>(NoTxRocksIterator<'a>, PhantomData<(K, V)>);

impl<'a, K, V> NoTxEntryIterator<'a, K, V> {
    pub fn new(inner: NoTxRocksIterator<'a>) -> Self {
        Self(inner, Default::default())
    }
}

impl<'a, K, V> Iterator for NoTxEntryIterator<'a, K, V>
where
    K: From<Box<[u8]>>,
    V: From<Box<[u8]>>,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Result<(K, V), Error>> {
        match self.0.next() {
            Some(Ok((key, value))) => {
                let key_out = K::from(key);
                let value_out = V::from(value);

                Some(Ok((key_out, value_out)))
            }
            Some(Err(err)) => {
                tracing::error!(?err);
                Some(Err(Error::Rocks(err)))
            }
            None => None,
        }
    }
}

pub struct SnapshotEntryIterator<'a, K, V>(SnapshotRocksIterator<'a>, PhantomData<(K, V)>);

impl<'a, K, V> SnapshotEntryIterator<'a, K, V> {
    pub fn new(inner: SnapshotRocksIterator<'a>) -> Self {
        Self(inner, Default::default())
    }
}

impl<'a, K, V> Iterator for SnapshotEntryIterator<'a, K, V>
where
    K: From<Box<[u8]>>,
    V: From<Box<[u8]>>,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Result<(K, V), Error>> {
        match self.0.next() {
            Some(Ok((key, value))) => {
                let key_out = K::from(key);
                let value_out = V::from(value);

                Some(Ok((key_out, value_out)))
            }
            Some(Err(err)) => {
                tracing::error!(?err);
                Some(Err(Error::Rocks(err)))
            }
            None => None,
        }
    }
}

pub trait KVTable<K, V>
where
    Box<[u8]>: From<K>,
    Box<[u8]>: From<V>,
    K: From<Box<[u8]>>,
    V: From<Box<[u8]>>,
{
    const CF_NAME: &'static str;

    fn cf(db: &rocksdb::TransactionDB) -> rocksdb::ColumnFamilyRef<'_> {
        db.cf_handle(Self::CF_NAME).unwrap()
    }

    fn reset(db: &rocksdb::TransactionDB) -> Result<(), Error> {
        db.drop_cf(Self::CF_NAME).map_err(Error::Rocks)?;

        let db_options = crate::storage::options::db_options();

        db.create_cf(Self::CF_NAME, &db_options)
            .map_err(Error::Rocks)?;

        Ok(())
    }

    fn get_by_key(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
        k: K,
    ) -> Result<Option<V>, Error> {
        let cf = Self::cf(db);
        let raw_key = Box::<[u8]>::from(k);
        let raw_value = tx
            .get_cf(&cf, raw_key)
            .map_err(Error::Rocks)?
            .map(|x| Box::from(x.as_slice()));

        match raw_value {
            Some(x) => {
                let out = <V>::from(x);
                Ok(Some(out))
            }
            None => Ok(None),
        }
    }

    fn stage_upsert(
        db: &TransactionDB,
        k: K,
        v: V,
        tx: &mut rocksdb::Transaction<TransactionDB>,
    ) -> Result<(), Error> {
        let cf = Self::cf(&db);

        let k_raw = Box::<[u8]>::from(k);
        let v_raw = Box::<[u8]>::from(v);

        tx.put_cf(&cf, k_raw, v_raw).map_err(Error::Rocks)
    }

    fn is_empty(db: &rocksdb::TransactionDB, tx: &rocksdb::Transaction<TransactionDB>) -> bool {
        let mut iter = Self::iter_keys(db, tx, rocksdb::IteratorMode::Start);
        iter.next().is_none()
    }

    fn iter_keys<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<TransactionDB>,
        mode: rocksdb::IteratorMode,
    ) -> KeyIterator<'a, K> {
        let cf = Self::cf(db);
        let inner = tx.iterator_cf(&cf, mode);
        KeyIterator::new(inner)
    }

    fn iter_keys_start<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
    ) -> KeyIterator<'a, K> {
        Self::iter_keys(db, tx, rocksdb::IteratorMode::Start)
    }

    fn iter_keys_from<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
        from: K,
    ) -> KeyIterator<'a, K> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Forward);

        Self::iter_keys(db, tx, mode)
    }

    fn iter_values<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
        mode: rocksdb::IteratorMode,
    ) -> ValueIterator<'a, V> {
        let cf = Self::cf(db);
        let inner = tx.iterator_cf(&cf, mode);
        ValueIterator::new(inner)
    }

    fn iter_values_start<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
    ) -> ValueIterator<'a, V> {
        Self::iter_values(db, tx, rocksdb::IteratorMode::Start)
    }

    fn iter_values_from<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
        from: K,
    ) -> ValueIterator<'a, V> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Forward);

        Self::iter_values(db, tx, mode)
    }

    fn iter_values_from_reverse<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
        from: K,
    ) -> ValueIterator<'a, V> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Reverse);

        Self::iter_values(db, tx, mode)
    }

    fn iter_entries<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<TransactionDB>,
        mode: rocksdb::IteratorMode,
    ) -> EntryIterator<'a, K, V> {
        let cf = Self::cf(db);
        let inner = tx.iterator_cf(&cf, mode);
        EntryIterator::new(inner)
    }

    fn iter_entries_snapshot<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::SnapshotWithThreadMode<TransactionDB>,
        mode: rocksdb::IteratorMode,
    ) -> SnapshotEntryIterator<'a, K, V> {
        let cf = Self::cf(db);
        let inner = tx.iterator_cf(&cf, mode);
        SnapshotEntryIterator::new(inner)
    }

    fn iter_entries_no_tx<'a>(
        db: &'a rocksdb::TransactionDB,
        mode: rocksdb::IteratorMode,
    ) -> NoTxEntryIterator<'a, K, V> {
        let cf = Self::cf(db);
        let inner = db.iterator_cf(&cf, mode);
        NoTxEntryIterator::new(inner)
    }

    fn iter_entries_start<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
    ) -> EntryIterator<'a, K, V> {
        Self::iter_entries(db, tx, rocksdb::IteratorMode::Start)
    }

    fn iter_entries_from<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::Transaction<'a, TransactionDB>,
        from: K,
    ) -> EntryIterator<'a, K, V> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Forward);

        Self::iter_entries(db, tx, mode)
    }

    fn iter_entries_from_snapshot<'a>(
        db: &'a rocksdb::TransactionDB,
        tx: &'a rocksdb::SnapshotWithThreadMode<'a, TransactionDB>,
        from: K,
    ) -> SnapshotEntryIterator<'a, K, V> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Forward);

        Self::iter_entries_snapshot(db, tx, mode)
    }

    fn iter_entries_from_no_tx<'a>(
        db: &'a rocksdb::TransactionDB,
        from: K,
    ) -> NoTxEntryIterator<'a, K, V> {
        let from_raw = Box::<[u8]>::from(from);
        let mode = rocksdb::IteratorMode::From(&from_raw, rocksdb::Direction::Forward);

        Self::iter_entries_no_tx(db, mode)
    }

    fn last_key(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
    ) -> Result<Option<K>, Error> {
        let mut iter = Self::iter_keys(db, tx, rocksdb::IteratorMode::End);

        match iter.next() {
            None => Ok(None),
            Some(x) => Ok(Some(x?)),
        }
    }

    fn last_value(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
    ) -> Result<Option<V>, Error> {
        let mut iter = Self::iter_values(db, tx, rocksdb::IteratorMode::End);

        match iter.next() {
            None => Ok(None),
            Some(x) => Ok(Some(x?)),
        }
    }

    fn last_entry(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
    ) -> Result<Option<(K, V)>, Error> {
        let mut iter = Self::iter_entries(db, tx, rocksdb::IteratorMode::End);

        match iter.next() {
            None => Ok(None),
            Some(x) => Ok(Some(x?)),
        }
    }

    fn scan_until<F>(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
        mode: rocksdb::IteratorMode,
        predicate: F,
    ) -> Result<Option<K>, Error>
    where
        F: Fn(&V) -> bool,
    {
        for entry in Self::iter_entries(db, tx, mode) {
            let (k, v) = entry?;

            if predicate(&v) {
                return Ok(Some(k));
            }
        }

        Ok(None)
    }

    fn scan_until_or<F, G>(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
        mode: rocksdb::IteratorMode,
        predicate: F,
        bail_predicate: G,
    ) -> Result<Option<K>, Error>
    where
        F: Fn(&V) -> bool,
        G: Fn(&V) -> bool,
    {
        for entry in Self::iter_entries(db, tx, mode) {
            let (k, v) = entry?;

            if predicate(&v) {
                return Ok(Some(k));
            }

            if bail_predicate(&v) {
                return Ok(None);
            }
        }

        Ok(None)
    }

    fn stage_delete(
        db: &rocksdb::TransactionDB,
        key: K,
        tx: &rocksdb::Transaction<TransactionDB>,
    ) -> Result<(), Error> {
        let cf = Self::cf(db);
        let k_raw = Box::<[u8]>::from(key);
        tx.delete_cf(&cf, k_raw).map_err(Error::Rocks)
    }
}

pub struct AnyTable;

pub trait AnyValue: Serialize + DeserializeOwned {
    fn type_key() -> DBInt;
}

impl KVTable<DBInt, DBBytes> for AnyTable {
    const CF_NAME: &'static str = "DefaultKV";
}

impl AnyTable {
    pub fn stage_upsert_any<T: AnyValue>(
        db: &rocksdb::TransactionDB,
        v: T,
        tx: &mut rocksdb::Transaction<TransactionDB>,
    ) -> Result<(), Error> {
        let k = T::type_key();
        let v = DBSerde(v).into();
        Self::stage_upsert(db, k, v, tx)
    }

    pub fn get<T: AnyValue>(
        db: &rocksdb::TransactionDB,
        tx: &rocksdb::Transaction<TransactionDB>,
    ) -> Result<Option<T>, Error> {
        let k = T::type_key();
        let v: Option<T> = AnyTable::get_by_key(db, tx, k)?
            .map(DBSerde::<T>::from)
            .map(|x| x.0);

        Ok(v)
    }
}
