use std::collections::BTreeMap;

use planner::Planner;

use crate::error::Result;

use super::{
    engine::Transaction,
    executor::{Executor, ResultSet},
    parser::ast::{self, Expression, OrderDirection},
    schema::Table, types::Value,
};

mod planner;

// 执行节点
#[derive(Debug, PartialEq)]
pub enum Node {
    // 创建表
    CreateTable {
        schema: Table,
    },

    // 插入数据
    Insert {
        table_name: String,
        columns: Vec<String>,
        values: Vec<Vec<Expression>>,
    },

    // 扫描节点
    Scan {
        table_name: String,
        filter: Option<Expression>,
    },

    // 更新节点
    Update {
        table_name: String,
        source: Box<Node>,
        columns: BTreeMap<String, Expression>,
    },

    // 删除节点
    Delete {
        table_name: String,
        source: Box<Node>,
    },

    // 排序节点
    Order {
        source: Box<Node>,
        order_by: Vec<(String, OrderDirection)>,
    },

    // Limit 节点
    Limit {
        source: Box<Node>,
        limit: usize,
    },

    // Offset 节点
    Offset {
        source: Box<Node>,
        offset: usize,
    },

    // 投影节点
    Projection {
        source: Box<Node>,
        select: Vec<(Expression, Option<String>)>,
    },

    // 嵌套循环 join 节点
    NestedLoopJoin {
        left: Box<Node>,
        right: Box<Node>,
        predicate: Option<Expression>,
        outer: bool,
    },

    // 聚集节点
    Aggregate {
        source: Box<Node>,
        exprs: Vec<(Expression, Option<String>)>,
        group_by: Option<Expression>,
    },

    // 过滤节点
    Filter {
        source: Box<Node>,
        predicate: Option<Expression>,
    },

    // 索引查询节点
    IndexScan {
        table_name: String,
        field: String,
        value: Value,
    },

    // 主键查询节点
    PrimaryKeyScan {
        table_name: String,
        value: Value,
    },
}

#[derive(Debug, PartialEq)]
pub struct Plan(pub Node);

impl Plan {
    pub fn build<T: Transaction>(stmt: ast::Statement, txn: &mut T) -> Result<Self> {
        Planner::new(txn).build(stmt)
    }

    pub fn execute<T: Transaction + 'static>(self, txn: &mut T) -> Result<ResultSet> {
        <dyn Executor<T>>::build(self.0).execute(txn)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::Result,
        sql::{
            engine::{kv::KVEngine, Engine}, parser::{
                ast::{self, Expression},
                Parser,
            }, plan::{Node, Plan}
        }, storage::disk::DiskEngine,
    };

    #[test]
    fn test_plan_create_table() -> Result<()> {
        let p = tempfile::tempdir()?.into_path().join("sqldb-log");
        let kvengine = KVEngine::new(DiskEngine::new(p.clone())?);
        let mut txn = kvengine.begin()?;

        let sql1 = "
        create table tbl1 (
            a int default 100,
            b float not null,
            c varchar null,
            d bool default true
        );
        ";
        let stmt1 = Parser::new(sql1).parse()?;
        let p1 = Plan::build(stmt1, &mut txn);

        let sql2 = "
        create            table tbl1 (
            a int default     100,
            b float not null     ,
            c varchar      null,
            d       bool default        true
        );
        ";
        let stmt2 = Parser::new(sql2).parse()?;
        let p2 = Plan::build(stmt2, &mut txn);
        assert_eq!(p1, p2);

        Ok(())
    }

    #[test]
    fn test_plan_insert() -> Result<()> {
        let p = tempfile::tempdir()?.into_path().join("sqldb-log");
        let kvengine = KVEngine::new(DiskEngine::new(p.clone())?);
        let mut txn = kvengine.begin()?;

        let sql1 = "insert into tbl1 values (1, 2, 3, 'a', true);";
        let stmt1 = Parser::new(sql1).parse()?;
        let p1 = Plan::build(stmt1, &mut txn)?;
        assert_eq!(
            p1,
            Plan(Node::Insert {
                table_name: "tbl1".to_string(),
                columns: vec![],
                values: vec![vec![
                    Expression::Consts(ast::Consts::Integer(1)),
                    Expression::Consts(ast::Consts::Integer(2)),
                    Expression::Consts(ast::Consts::Integer(3)),
                    Expression::Consts(ast::Consts::String("a".to_string())),
                    Expression::Consts(ast::Consts::Boolean(true)),
                ]],
            })
        );

        let sql2 = "insert into tbl2 (c1, c2, c3) values (3, 'a', true),(4, 'b', false);";
        let stmt2 = Parser::new(sql2).parse()?;
        let p2 = Plan::build(stmt2, &mut txn)?;
        assert_eq!(
            p2,
            Plan(Node::Insert {
                table_name: "tbl2".to_string(),
                columns: vec!["c1".to_string(), "c2".to_string(), "c3".to_string()],
                values: vec![
                    vec![
                        Expression::Consts(ast::Consts::Integer(3)),
                        Expression::Consts(ast::Consts::String("a".to_string())),
                        Expression::Consts(ast::Consts::Boolean(true)),
                    ],
                    vec![
                        Expression::Consts(ast::Consts::Integer(4)),
                        Expression::Consts(ast::Consts::String("b".to_string())),
                        Expression::Consts(ast::Consts::Boolean(false)),
                    ],
                ],
            })
        );

        Ok(())
    }

    #[test]
    fn test_plan_select() -> Result<()> {
        let p = tempfile::tempdir()?.into_path().join("sqldb-log");
        let kvengine = KVEngine::new(DiskEngine::new(p.clone())?);
        let mut txn = kvengine.begin()?;

        let sql = "select * from tbl1;";
        let stmt = Parser::new(sql).parse()?;
        let p = Plan::build(stmt, &mut txn)?;
        assert_eq!(
            p,
            Plan(Node::Scan {
                table_name: "tbl1".to_string(),
                filter: None
            })
        );

        Ok(())
    }
}
