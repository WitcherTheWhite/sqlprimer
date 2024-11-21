use crate::error::Result;

use super::{executor::ResultSet, parser::Parser, plan::Plan, schema::Table, types::Row};

mod kv;

// 抽象的 SQL 引擎层定义
pub trait Engine: Clone {
    type Transaction: Transaction;

    fn begin(&self) -> Result<Self::Transaction>;

    fn session(&self) -> Result<Session<Self>> {
        Ok(Session {
            engine: self.clone(),
        })
    }
}

// 抽象事务信息，底层可以接入不同的存储引擎
pub trait Transaction {
    fn commmit(&self) -> Result<()>;

    fn rollback(&self) -> Result<()>;

    fn create_row(&mut self, table: String, row: Row) -> Result<()>;

    fn scan_table(&self, table_name: String) -> Result<Vec<Row>>;

    fn create_table(&mut self, table: Table) -> Result<()>;

    fn get_table(&self, table_name: String) -> Result<Option<Table>>;
}

pub struct Session<E: Engine> {
    engine: E,
}

impl<E: Engine> Session<E> {
    // 执行客户端 SQL 语句
    pub fn execute(&mut self, sql: &str) -> Result<ResultSet> {
        match Parser::new(sql).parse()? {
            stmt => {
                let mut txn = self.engine.begin()?;
                // 构建 Plan，执行 SQL 语句
                match Plan::build(stmt).execute(&mut txn) {
                    Ok(result) => {
                        txn.commmit()?;
                        Ok(result)
                    }
                    Err(err) => {
                        txn.rollback()?;
                        Err(err)
                    }
                }
            }
        }
    }
}
