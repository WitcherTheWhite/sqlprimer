use crate::{error::Result, storage};

use super::{Engine, Transaction};

// KV Engine 定义
pub struct KVEngine {
    pub kv: storage::Mvcc,
}

impl Clone for KVEngine {
    fn clone(&self) -> Self {
        Self {
            kv: self.kv.clone(),
        }
    }
}

impl Engine for KVEngine {
    type Transaction = KVTransaction;

    fn begin(&self) -> Result<Self::Transaction> {
        Ok(KVTransaction::new(self.kv.begin()?))
    }
}

pub struct KVTransaction {
    txn: storage::MvccTransaciton,
}

impl KVTransaction {
    pub fn new(txn: storage::MvccTransaciton) -> Self {
        Self { txn }
    }
}

impl Transaction for KVTransaction {
    fn commmit(&self) -> Result<()> {
        todo!()
    }

    fn rollback(&self) -> Result<()> {
        todo!()
    }

    fn create_row(&mut self, table: String, row: crate::sql::types::Row) -> Result<()> {
        todo!()
    }

    fn scan_table(&self, table_name: String) -> Result<Vec<crate::sql::types::Row>> {
        todo!()
    }

    fn create_table(&mut self, table: crate::sql::schema::Table) -> Result<()> {
        todo!()
    }

    fn get_table(&self, table_name: String) -> Result<Option<crate::sql::schema::Table>> {
        todo!()
    }
}
