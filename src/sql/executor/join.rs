use crate::{
    error::{Error, Result},
    sql::{
        engine::Transaction,
        parser::ast::{self, Expression},
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

fn evaluate_expr(
    expr: &Expression,
    lcols: &Vec<String>,
    lrow: &Vec<Value>,
    rcols: &Vec<String>,
    rrow: &Vec<Value>,
) -> Result<Value> {
    match expr {
        Expression::Filed(col_name) => {
            let pos = match lcols.iter().position(|c| *c == *col_name) {
                Some(pos) => pos,
                None => {
                    return Err(Error::Internal(format!(
                        "column {} is not in table",
                        col_name
                    )))
                }
            };
            Ok(lrow[pos].clone())
        }
        Expression::Operation(operation) => match operation {
            ast::Operation::Equal(lexpr, rexpr) => {
                let lv = evaluate_expr(&lexpr, lcols, lrow, rcols, rrow)?;
                let rv = evaluate_expr(&rexpr, rcols, rrow, lcols, lrow)?;
                Ok(match (lv, rv) {
                    (Value::Null, _) => Value::Null,
                    (_, Value::Null) => Value::Null,
                    (Value::Boolean(l), Value::Boolean(r)) => Value::Boolean(l == r),
                    (Value::Integer(l), Value::Integer(r)) => Value::Boolean(l == r),
                    (Value::Integer(l), Value::Float(r)) => Value::Boolean(l as f64 == r),
                    (Value::Float(l), Value::Integer(r)) => Value::Boolean(l == r as f64),
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l == r),
                    (Value::String(l), Value::String(r)) => Value::Boolean(l == r),
                    (l, r) => {
                        return Err(Error::Internal(format!(
                            "cannot compare expression {} and {}",
                            l, r
                        )))
                    }
                })
            }
        },
        _ => Err(Error::Internal("Unexpected expression".into())),
    }
}
