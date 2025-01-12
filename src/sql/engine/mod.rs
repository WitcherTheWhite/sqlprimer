use crate::error::{Error, Result};

use super::{
    executor::ResultSet,
    parser::{
        ast::{self, Expression},
        Parser,
    },
    plan::Plan,
    schema::Table,
    types::{Row, Value},
};

pub mod kv;

// 抽象的 SQL 引擎层定义
pub trait Engine: Clone {
    type Transaction: Transaction;

    fn begin(&self) -> Result<Self::Transaction>;

    fn session(&self) -> Result<Session<Self>> {
        Ok(Session {
            engine: self.clone(),
            txn: None,
        })
    }
}

// 抽象事务信息，底层可以接入不同的存储引擎
pub trait Transaction {
    fn commmit(&self) -> Result<()>;

    fn rollback(&self) -> Result<()>;

    fn version(&self) -> u64;

    fn create_row(&mut self, table: String, row: Row) -> Result<()>;

    fn update_row(&mut self, table: &Table, id: &Value, row: Row) -> Result<()>;

    fn delete_row(&mut self, table: &Table, row: Row) -> Result<()>;

    fn scan_table(&self, table_name: String, filter: Option<Expression>) -> Result<Vec<Row>>;

    fn get_table_names(&self) -> Result<Vec<String>>;

    fn create_table(&mut self, table: Table) -> Result<()>;

    fn get_table(&self, table_name: String) -> Result<Option<Table>>;

    fn must_get_table(&self, table_name: String) -> Result<Table> {
        self.get_table(table_name.clone())?
            .ok_or(Error::Internal(format!(
                "table {} does not exist",
                table_name
            )))
    }
}

pub struct Session<E: Engine> {
    engine: E,
    txn: Option<E::Transaction>,
}

impl<E: Engine + 'static> Session<E> {
    // 执行客户端 SQL 语句
    pub fn execute(&mut self, sql: &str) -> Result<ResultSet> {
        match Parser::new(sql).parse()? {
            ast::Statement::Begin if self.txn.is_some() => {
                Err(Error::Internal("Already in transaction".into()))
            }
            ast::Statement::Commit | ast::Statement::Rollback if self.txn.is_none() => {
                Err(Error::Internal("Not in transaction".into()))
            }
            ast::Statement::Begin => {
                let txn = self.engine.begin()?;
                let version = txn.version();
                self.txn = Some(txn);
                Ok(ResultSet::Begin { version })
            }
            ast::Statement::Commit => {
                let mut version = 0;
                if let Some(txn) = &self.txn {
                    version = txn.version();
                    txn.commmit()?;
                    self.txn = None;
                }
                Ok(ResultSet::Commit { version })
            }
            ast::Statement::Rollback => {
                let mut version = 0;
                if let Some(txn) = &self.txn {
                    version = txn.version();
                    txn.rollback()?;
                    self.txn = None;
                }
                Ok(ResultSet::Rollback { version })
            }
            stmt if self.txn.is_some() => Plan::build(stmt)?.execute(self.txn.as_mut().unwrap()),
            stmt => {
                let mut txn = self.engine.begin()?;
                // 构建 Plan，执行 SQL 语句
                match Plan::build(stmt)?.execute(&mut txn) {
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

    pub fn get_table(&mut self, table_name: String) -> Result<String> {
        let txn = self.engine.begin()?;
        let table = txn.must_get_table(table_name)?;
        txn.commmit()?;
        Ok(table.to_string())
    }

    pub fn get_table_names(&mut self) -> Result<String> {
        let txn = self.engine.begin()?;
        let table_names = txn.get_table_names()?;
        txn.commmit()?;
        Ok(table_names.join("\n"))
    }
}
