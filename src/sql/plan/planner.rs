use crate::{
    error::{Error, Result},
    sql::{
        parser::ast::{self, Expression},
        schema::{self, Table},
        types::Value,
    },
};

use super::{Node, Plan};

pub struct Planner;

impl Planner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(&self, stmt: ast::Statement) -> Result<Plan> {
        Ok(Plan(self.build_statement(stmt)?))
    }

    fn build_statement(&self, stmt: ast::Statement) -> Result<Node> {
        Ok(match stmt {
            ast::Statement::CreateTable { name, columns } => Node::CreateTable {
                schema: Table {
                    name,
                    columns: columns
                        .into_iter()
                        .map(|c| {
                            let nullable = c.nullable.unwrap_or(!c.primary_key);
                            let default = match c.default {
                                Some(expr) => Some(Value::from_expression(expr)),
                                None if nullable => Some(Value::Null),
                                None => None,
                            };

                            schema::Column {
                                name: c.name,
                                datatype: c.datatype,
                                nullable,
                                default,
                                primary_key: c.primary_key,
                                index: true,
                            }
                        })
                        .collect(),
                },
            },
            ast::Statement::Insert {
                table_name,
                columns,
                values,
            } => Node::Insert {
                table_name,
                columns: columns.unwrap_or_default(),
                values,
            },
            ast::Statement::Select {
                order_by,
                limit,
                offset,
                select,
                from,
                group_by,
                where_clause,
                having,
            } => {
                let mut node = self.build_from_item(from, where_clause)?;

                let mut has_agg = false;
                if !select.is_empty() {
                    for (expr, _) in select.iter() {
                        if let Expression::Function(_, _) = expr {
                            has_agg = true;
                            break;
                        }
                    }
                    if has_agg {
                        node = Node::Aggregate {
                            source: Box::new(node),
                            exprs: select.clone(),
                            group_by,
                        }
                    }
                }

                if having.is_some() {
                    node = Node::Filter {
                        source: Box::new(node),
                        predicate: having,
                    }
                }

                if !order_by.is_empty() {
                    node = Node::Order {
                        source: Box::new(node),
                        order_by,
                    }
                }

                if let Some(expr) = offset {
                    node = Node::Offset {
                        source: Box::new(node),
                        offset: match Value::from_expression(expr) {
                            Value::Integer(i) => i as usize,
                            _ => return Err(Error::Internal("invalid offset".into())),
                        },
                    }
                }

                if let Some(expr) = limit {
                    node = Node::Limit {
                        source: Box::new(node),
                        limit: match Value::from_expression(expr) {
                            Value::Integer(i) => i as usize,
                            _ => return Err(Error::Internal("invalid limit".into())),
                        },
                    }
                }

                if !select.is_empty() && !has_agg {
                    node = Node::Projection {
                        source: Box::new(node),
                        select,
                    }
                }

                node
            }
            ast::Statement::Update {
                table_name,
                columns,
                where_clause,
            } => Node::Update {
                table_name: table_name.clone(),
                source: Box::new(Node::Scan {
                    table_name: table_name,
                    filter: where_clause,
                }),
                columns,
            },
            ast::Statement::Delete {
                table_name,
                where_clause,
            } => Node::Delete {
                table_name: table_name.clone(),
                source: Box::new(Node::Scan {
                    table_name: table_name,
                    filter: where_clause,
                }),
            },
            ast::Statement::Begin | ast::Statement::Commit | ast::Statement::Rollback => {
                return Err(Error::Internal("unexpected transaction command".into()));
            }
        })
    }

    fn build_from_item(&self, item: ast::FromItem, filter: Option<Expression>) -> Result<Node> {
        match item {
            ast::FromItem::Table { name } => Ok(Node::Scan {
                table_name: name,
                filter: filter,
            }),
            ast::FromItem::Join {
                left,
                right,
                join_type,
                predicate,
            } => {
                // 把 right join 转换为 left join
                let (left, right) = match join_type {
                    ast::JoinType::Right => (right, left),
                    _ => (left, right),
                };

                let outer = match join_type {
                    ast::JoinType::Cross | ast::JoinType::Inner => false,
                    _ => true,
                };

                Ok(Node::NestedLoopJoin {
                    left: Box::new(self.build_from_item(*left, filter.clone())?),
                    right: Box::new(self.build_from_item(*right, filter)?),
                    predicate,
                    outer,
                })
            }
        }
    }
}
