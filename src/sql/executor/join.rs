use std::collections::HashMap;

use crate::{
    error::{Error, Result},
    sql::{
        engine::Transaction,
        parser::ast::{evaluate_expr, Expression, Operation},
        types::Value,
    },
};

use super::{Executor, ResultSet};

pub struct NestedLoopJoin<T: Transaction> {
    left: Box<dyn Executor<T>>,
    right: Box<dyn Executor<T>>,
    predicate: Option<Expression>,
    outer: bool,
}

impl<T: Transaction> NestedLoopJoin<T> {
    pub fn new(
        left: Box<dyn Executor<T>>,
        right: Box<dyn Executor<T>>,
        predicate: Option<Expression>,
        outer: bool,
    ) -> Box<Self> {
        Box::new(Self {
            left,
            right,
            predicate,
            outer,
        })
    }
}

impl<T: Transaction> Executor<T> for NestedLoopJoin<T> {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        if let ResultSet::Scan {
            columns: lcols,
            rows: lrows,
        } = self.left.execute(txn)?
        {
            let mut new_rows = Vec::new();
            let mut new_cols = lcols.clone();
            if let ResultSet::Scan {
                columns: rcols,
                rows: rrows,
            } = self.right.execute(txn)?
            {
                new_cols.extend(rcols.clone());

                for lrow in &lrows {
                    let mut mathched = false;

                    for rrow in &rrows {
                        // 判断是否满足 join 条件
                        if let Some(expr) = &self.predicate {
                            match evaluate_expr(expr, &lcols, lrow, &rcols, rrow)? {
                                Value::Null => {}
                                Value::Boolean(false) => {}
                                Value::Boolean(true) => {
                                    let mut row = lrow.clone();
                                    row.extend(rrow.clone());
                                    new_rows.push(row);
                                    mathched = true;
                                }
                                _ => return Err(Error::Internal("Unexpected expression".into())),
                            }
                        } else {
                            let mut row = lrow.clone();
                            row.extend(rrow.clone());
                            new_rows.push(row);
                        }
                    }

                    // 没有匹配则用 NULL 值填充
                    if self.outer && !mathched {
                        let mut row = lrow.clone();
                        for _ in 0..rcols.len() {
                            row.push(Value::Null);
                        }
                        new_rows.push(row);
                    }
                }

                return Ok(ResultSet::Scan {
                    columns: new_cols,
                    rows: new_rows,
                });
            }
        }

        Err(Error::Internal("Unexpected result set".into()))
    }
}

pub struct HashJoin<T: Transaction> {
    left: Box<dyn Executor<T>>,
    right: Box<dyn Executor<T>>,
    predicate: Option<Expression>,
    outer: bool,
}

impl<T: Transaction> HashJoin<T> {
    pub fn new(
        left: Box<dyn Executor<T>>,
        right: Box<dyn Executor<T>>,
        predicate: Option<Expression>,
        outer: bool,
    ) -> Box<Self> {
        Box::new(Self {
            left,
            right,
            predicate,
            outer,
        })
    }
}

impl<T: Transaction> Executor<T> for HashJoin<T> {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        if let ResultSet::Scan {
            columns: lcols,
            rows: lrows,
        } = self.left.execute(txn)?
        {
            let mut new_rows = Vec::new();
            let mut new_cols = lcols.clone();
            if let ResultSet::Scan {
                columns: rcols,
                rows: rrows,
            } = self.right.execute(txn)?
            {
                new_cols.extend(rcols.clone());
                let (lcol_name, rcol_name) = match parse_join(self.predicate) {
                    Some((l, r)) => (l, r),
                    None => return Err(Error::Internal("unexpected expression".into())),
                };

                // 以 join 列的值作为 key 保存它所对应的行序号
                let rcol_pos = match rcols.iter().position(|c| *c == rcol_name) {
                    Some(pos) => pos,
                    None => return Err(Error::Internal("Column {} is not in table".into())),
                };
                let mut join_map = HashMap::new();
                for (i, row) in rrows.iter().enumerate() {
                    let key = row[rcol_pos].clone();
                    let val = join_map.entry(key).or_insert(Vec::new());
                    val.push(i);
                }

                // 遍历所有左表的行找到右表匹配的行，外连接用 NULL 值填充
                let lcol_pos = match lcols.iter().position(|c| *c == lcol_name) {
                    Some(pos) => pos,
                    None => return Err(Error::Internal("Column {} is not in table".into())),
                };
                for lrow in &lrows {
                    let lvalue = &lrow[lcol_pos];
                    if let Some(indexs) = join_map.get(lvalue) {
                        for index in indexs {
                            let mut row = lrow.clone();
                            row.extend(rrows[*index].clone());
                            new_rows.push(row);
                        }
                    } else if self.outer {
                        let mut row = lrow.clone();
                        for _ in 0..rcols.len() {
                            row.push(Value::Null);
                        }
                        new_rows.push(row);
                    }
                }
            }

            return Ok(ResultSet::Scan {
                columns: new_cols,
                rows: new_rows,
            });
        }

        Err(Error::Internal("Unexpected result set".into()))
    }
}

// 解析 join 条件
fn parse_join(filter: Option<Expression>) -> Option<(String, String)> {
    match filter {
        Some(expr) => match expr {
            Expression::Filed(f) => Some((f, "".into())),
            Expression::Operation(operation) => match operation {
                Operation::Equal(l, r) => {
                    let lv = parse_join(Some(*l));
                    let rv = parse_join(Some(*r));

                    Some((lv.unwrap().0, rv.unwrap().0))
                }
                _ => None,
            },
            _ => None,
        },
        None => None,
    }
}
