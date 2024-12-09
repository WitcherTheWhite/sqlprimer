use crate::error::Result;

pub mod engine;
pub mod memory;

pub struct Mvcc {}

impl Clone for Mvcc {
    fn clone(&self) -> Self {
        Self {}
    }
}

impl Mvcc {
    pub fn new() -> Self {
        Self {}
    }

    pub fn begin(&self) -> Result<MvccTransaciton> {
        Ok(MvccTransaciton::new())
    }
}

pub struct MvccTransaciton {}

impl MvccTransaciton {
    pub fn new() -> Self {
        Self {}
    }
}
