use std::sync::{Arc, Mutex};

use crate::error::Result;

use super::engine::Engine;

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
        Ok(MvccTransaciton::begin(self.engine.clone()))
    }
}

pub struct MvccTransaciton<E: Engine> {
    engine: Arc<Mutex<E>>,
}

impl<E: Engine> MvccTransaciton<E> {
    pub fn begin(eng: Arc<Mutex<E>>) -> Self {
        Self { engine: eng }
    }

    pub fn commmit(&self) -> Result<()> {
        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        Ok(())
    }

    pub fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut eng = self.engine.lock()?;
        eng.set(key, value)
    }

    pub fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let mut eng = self.engine.lock()?;
        eng.get(key)
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
}

pub struct ScanResult {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
