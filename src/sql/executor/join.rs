use crate::{
    error::{Error, Result},
    sql::{
        engine::Transaction,
        parser::ast::{evaluate_expr, Expression},
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
