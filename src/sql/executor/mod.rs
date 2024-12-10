use mutation::Insert;
use query::Scan;
use schema::CreateTable;

use crate::error::Result;

use super::{engine::Transaction, plan::Node, types::Row};

mod mutation;
mod query;
mod schema;

// 执行器定义
pub trait Executor<T: Transaction> {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet>;
}

impl<T: Transaction> dyn Executor<T> {
    pub fn build(node: Node) -> Box<dyn Executor<T>> {
        match node {
            Node::CreateTable { schema } => CreateTable::new(schema),
            Node::Insert {
                table_name,
                columns,
                values,
            } => Insert::new(table_name, columns, values),
            Node::Scan { table_name } => Scan::new(table_name),
        }
    }
}

// 执行结果集
#[derive(Debug)]
pub enum ResultSet {
    CreateTable { table_name: String },
    Insert { count: usize },
    Scan { columns: Vec<String>, row: Vec<Row> },
}
