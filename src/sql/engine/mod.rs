use std::collections::HashSet;

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

    fn delete_row(&mut self, table: &Table, id: &Value) -> Result<()>;

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

    fn drop_table(&mut self, table_name: String) -> Result<()>;

    // 获取索引
    fn load_index(
        &self,
        table_name: &str,
        col_name: &str,
        col_value: &Value,
    ) -> Result<HashSet<Value>>;

    // 保存索引
    fn save_index(
        &self,
        table_name: &str,
        col_name: &str,
        col_value: &Value,
        index: HashSet<Value>,
    ) -> Result<()>;

    // 根据 id 获取行
    fn read_by_id(&self, table_name: &str, id: &Value) -> Result<Option<Row>>;
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
            ast::Statement::Explain { stmt } => {
                let plan = match self.txn.as_ref() {
                    Some(_) => Plan::build(*stmt, self.txn.as_mut().unwrap())?,
                    None => {
                        let mut txn = self.engine.begin()?;
                        let plan = Plan::build(*stmt, &mut txn)?;
                        txn.commmit()?;
                        plan
                    }
                };
                Ok(ResultSet::Explain {
                    plan: plan.0.to_string(),
                })
            }
            stmt if self.txn.is_some() => {
                Plan::build(stmt, self.txn.as_mut().unwrap())?.execute(self.txn.as_mut().unwrap())
            }
            stmt => {
                let mut txn = self.engine.begin()?;
                // 构建 Plan，执行 SQL 语句
                match Plan::build(stmt, &mut txn)?.execute(&mut txn) {
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
