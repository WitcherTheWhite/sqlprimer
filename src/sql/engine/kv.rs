use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    sql::{
        schema::Table,
        types::{Row, Value},
    },
    storage::{self, engine::Engine as StorageEngine},
};

use super::{Engine, Transaction};

// KV Engine 定义
pub struct KVEngine<E: StorageEngine> {
    pub kv: storage::mvcc::Mvcc<E>,
}

impl<E: StorageEngine> Clone for KVEngine<E> {
    fn clone(&self) -> Self {
        Self {
            kv: self.kv.clone(),
        }
    }
}

impl<E: StorageEngine> KVEngine<E> {
    pub fn new(engine: E) -> Self {
        Self {
            kv: storage::mvcc::Mvcc::new(engine),
        }
    }
}

impl<E: StorageEngine> Engine for KVEngine<E> {
    type Transaction = KVTransaction<E>;

    fn begin(&self) -> Result<Self::Transaction> {
        Ok(KVTransaction::new(self.kv.begin()?))
    }
}

pub struct KVTransaction<E: StorageEngine> {
    txn: storage::mvcc::MvccTransaciton<E>,
}

impl<E: StorageEngine> KVTransaction<E> {
    pub fn new(txn: storage::mvcc::MvccTransaciton<E>) -> Self {
        Self { txn }
    }
}

impl<E: StorageEngine> Transaction for KVTransaction<E> {
    fn commmit(&self) -> Result<()> {
        Ok(())
    }

    fn rollback(&self) -> Result<()> {
        Ok(())
    }

    fn create_row(&mut self, table_name: String, row: Row) -> Result<()> {
        let table = self.must_get_table(table_name.clone())?;
        // 检验行的有效性
        for (i, column) in table.columns.iter().enumerate() {
            match row[i].datatype() {
                None if column.nullable => {}
                None => {
                    return Err(Error::Internal(format!(
                        "column {} cannot be null",
                        column.name
                    )))
                }
                Some(dt) if dt != column.datatype => {
                    return Err(Error::Internal(format!(
                        "column {} type mismatch",
                        column.name
                    )))
                }
                _ => {}
            }
        }

        let id = Key::Row(table_name, row[0].clone());
        let key = bincode::serialize(&id)?;
        let value = bincode::serialize(&row)?;
        self.txn.set(key, value)?;

        Ok(())
    }

    fn scan_table(&self, table_name: String) -> Result<Vec<Row>> {
        let prefix = KeyPrefix::Row(table_name.clone());
        let results = self.txn.scan_prefix(bincode::serialize(&prefix)?)?;

        let mut rows = Vec::new();
        for result in results {
            let row: Row = bincode::deserialize(&result.value)?;
            rows.push(row);
        }

        Ok(rows)
    }

    fn create_table(&mut self, table: Table) -> Result<()> {
        // 判断表是否已经存在
        if self.get_table(table.name.clone())?.is_some() {
            return Err(Error::Internal(format!(
                "table {} already exists",
                table.name
            )));
        }

        // 判断表是否有效
        if table.columns.is_empty() {
            return Err(Error::Internal(format!(
                "table {} has no columns",
                table.name
            )));
        }

        let key = bincode::serialize(&Key::Table(table.name.clone()))?;
        let value = bincode::serialize(&table)?;
        self.txn.set(key, value)?;

        Ok(())
    }

    fn get_table(&self, table_name: String) -> Result<Option<Table>> {
        let key = bincode::serialize(&Key::Table(table_name))?;
        Ok(self
            .txn
            .get(key)?
            .map(|v| bincode::deserialize(&v))
            .transpose()?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Key {
    Table(String),
    Row(String, Value),
}

#[derive(Debug, Serialize, Deserialize)]
enum KeyPrefix {
    Table,
    Row(String),
}
#[cfg(test)]
mod tests {
    use crate::{error::Result, sql::engine::Engine, storage::memory::MemoryEngine};

    use super::KVEngine;

    #[test]
    fn test_create_talbe() -> Result<()> {
        let kv_engine = KVEngine::new(MemoryEngine::new());
        let mut s = kv_engine.session()?;
        s.execute("create table t1 (a int, b text default 'zz', c integer default 100);")?;
        s.execute("insert into t1 values (1, 'hsy', 5);")?;
        s.execute("insert into t1 values (2, 'a');")?;
        s.execute("insert into t1 (c, a) values (200, 3);")?;

        let v1 = s.execute("select * from t1;")?;
        println!("{:?}", v1);
        Ok(())
    }
}
