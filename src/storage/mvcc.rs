use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
    u64,
};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::engine::Engine;

pub type Version = u64;

pub struct Mvcc<E: Engine> {
    engine: Arc<Mutex<E>>,
}

impl<E: Engine> Clone for Mvcc<E> {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
        }
    }
}

impl<E: Engine> Mvcc<E> {
    pub fn new(eng: E) -> Self {
        Self {
            engine: Arc::new(Mutex::new(eng)),
        }
    }

    pub fn begin(&self) -> Result<MvccTransaciton<E>> {
        Ok(MvccTransaciton::begin(self.engine.clone())?)
    }
}

pub struct MvccTransaciton<E: Engine> {
    engine: Arc<Mutex<E>>,
    state: TransactionState,
}

// 事务状态
pub struct TransactionState {
    pub version: Version,
    pub active_versions: HashSet<Version>,
}

impl TransactionState {
    fn is_visible(&self, version: Version) -> bool {
        if self.active_versions.contains(&version) {
            return false;
        } else {
            return version <= self.version;
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum MvccKey {
    NextVersion,
    TxnActive(Version),
    TxnWrite(Version, #[serde(with = "serde_bytes")] Vec<u8>),
    Version(#[serde(with = "serde_bytes")] Vec<u8>, Version),
}

impl MvccKey {
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub fn decode(data: Vec<u8>) -> Result<Self> {
        Ok(bincode::deserialize(&data)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MvccKeyPrefix {
    NextVersion,
    TxnActive,
    TxnWrite(Version),
    Version(#[serde(with = "serde_bytes")] Vec<u8>),
}

impl MvccKeyPrefix {
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }
}

impl<E: Engine> MvccTransaciton<E> {
    pub fn begin(eng: Arc<Mutex<E>>) -> Result<Self> {
        let mut engine = eng.lock()?;
        // 获取最新版本号
        let next_version = match engine.get(MvccKey::NextVersion.encode())? {
            Some(value) => bincode::deserialize(&value)?,
            None => 1,
        };
        // 保存下一个版本号
        engine.set(
            MvccKey::NextVersion.encode(),
            bincode::serialize(&(next_version + 1))?,
        )?;

        // 获取当前活跃事务列表并将当前事务设置为活跃事务
        let active_versions = Self::scan_active(&mut engine)?;
        engine.set(MvccKey::TxnActive(next_version).encode(), vec![])?;

        Ok(Self {
            engine: eng.clone(),
            state: TransactionState {
                version: next_version,
                active_versions,
            },
        })
    }

    pub fn commmit(&self) -> Result<()> {
        let mut engine = self.engine.lock()?;

        // 删除当前事务的已写入 key 信息
        let mut delete_keys = Vec::new();
        let mut iter = engine.scan_prefix(MvccKeyPrefix::TxnWrite(self.state.version).encode());
        while let Some((key, _)) = iter.next().transpose()? {
            delete_keys.push(key);
        }
        drop(iter);

        for key in delete_keys.into_iter() {
            engine.delete(key)?;
        }

        // 当前事务不再是活跃事务
        engine.delete(MvccKey::TxnActive(self.state.version).encode())?;

        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        let mut engine = self.engine.lock()?;

        // 删除当前事务的已写入 key 信息和实际写入的 key/value 数据
        let mut delete_keys = Vec::new();
        let mut iter = engine.scan_prefix(MvccKeyPrefix::TxnWrite(self.state.version).encode());
        while let Some((key, _)) = iter.next().transpose()? {
            match MvccKey::decode(key.clone())? {
                MvccKey::TxnWrite(_, raw_key) => {
                    delete_keys.push(MvccKey::Version(raw_key, self.state.version).encode());
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "unexpected key: {:?}",
                        String::from_utf8(key)
                    )))
                }
            }
            delete_keys.push(key);
        }
        drop(iter);

        for key in delete_keys.into_iter() {
            engine.delete(key)?;
        }

        // 当前事务不再是活跃事务
        engine.delete(MvccKey::TxnActive(self.state.version).encode())?;
        Ok(())
    }

    pub fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.write_inner(key, Some(value))
    }

    pub fn delete(&self, key: Vec<u8>) -> Result<()> {
        self.write_inner(key, None)
    }

    pub fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let mut engine = self.engine.lock()?;

        let from = MvccKey::Version(key.clone(), 0).encode();
        let to = MvccKey::Version(key.clone(), self.state.version).encode();
        let mut iter = engine.scan(from..=to).rev();
        // 找到最新的一个可见版本值
        while let Some((key, value)) = iter.next().transpose()? {
            match MvccKey::decode(key.clone())? {
                MvccKey::Version(_, version) => {
                    if self.state.is_visible(version) {
                        return Ok(Some(bincode::deserialize(&value)?));
                    }
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "unexpected key: {:?}",
                        String::from_utf8(key)
                    )))
                }
            }
        }

        Ok(None)
    }

    pub fn scan_prefix(&self, prefix: Vec<u8>) -> Result<Vec<ScanResult>> {
        let mut eng = self.engine.lock()?;
        let mut iter = eng.scan_prefix(prefix);
        let mut results = Vec::new();
        while let Some((key, value)) = iter.next().transpose()? {
            results.push(ScanResult { key, value });
        }

        Ok(results)
    }

    // 写入/删除数据
    fn write_inner(&self, key: Vec<u8>, value: Option<Vec<u8>>) -> Result<()> {
        let mut engine = self.engine.lock()?;

        // 检测冲突
        let from = MvccKey::Version(
            key.clone(),
            self.state
                .active_versions
                .iter()
                .min()
                .copied()
                .unwrap_or(self.state.version + 1),
        )
        .encode();
        let to = MvccKey::Version(key.clone(), u64::MAX).encode();
        if let Some((k, _)) = engine.scan(from..=to).last().transpose()? {
            match MvccKey::decode(k.clone())? {
                MvccKey::Version(_, version) => {
                    // 判断 version 是否可见
                    if !self.state.is_visible(version) {
                        return Err(Error::WriteConflict);
                    }
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "unexpected key: {:?}",
                        String::from_utf8(key)
                    )));
                }
            }
        }

        // 记录当前事务写入的 key
        engine.set(
            MvccKey::TxnWrite(self.state.version, key.clone()).encode(),
            vec![],
        )?;

        // 写入实际的 key/value 数据
        engine.set(
            MvccKey::Version(key, self.state.version).encode(),
            bincode::serialize(&value)?,
        )?;

        Ok(())
    }

    // 扫描当前活跃事务列表
    pub fn scan_active(engine: &mut MutexGuard<E>) -> Result<HashSet<Version>> {
        let mut active_versions = HashSet::new();
        let mut iter = engine.scan_prefix(MvccKeyPrefix::TxnActive.encode());
        while let Some((key, _)) = iter.next().transpose()? {
            match MvccKey::decode(key.clone())? {
                MvccKey::TxnActive(version) => {
                    active_versions.insert(version);
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "unexpected key: {:?}",
                        String::from_utf8(key)
                    )));
                }
            }
        }

        Ok(active_versions)
    }
}

pub struct ScanResult {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
