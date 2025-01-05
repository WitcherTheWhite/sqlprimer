use std::{cmp::Ordering, collections::HashMap};

use crate::{
    error::{Error, Result},
    sql::{
        engine::Transaction,
        parser::ast::{Expression, OrderDirection},
    },
};

use super::{Executor, ResultSet};

pub struct Scan {
    table_name: String,
    filter: Option<(String, Expression)>,
}

impl Scan {
    pub fn new(table_name: String, filter: Option<(String, Expression)>) -> Box<Self> {
        Box::new(Self { table_name, filter })
    }
}

impl<T: Transaction> Executor<T> for Scan {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        let table = txn.must_get_table(self.table_name.clone())?;
        let rows = txn.scan_table(self.table_name, self.filter)?;

        Ok(ResultSet::Scan {
            columns: table
                .columns
                .into_iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
            rows,
        })
    }
}

pub struct Order<T: Transaction> {
    source: Box<dyn Executor<T>>,
    order_by: Vec<(String, OrderDirection)>,
}

impl<T: Transaction> Order<T> {
    pub fn new(source: Box<dyn Executor<T>>, order_by: Vec<(String, OrderDirection)>) -> Box<Self> {
        Box::new(Self { source, order_by })
    }
}

impl<T: Transaction> Executor<T> for Order<T> {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        match self.source.execute(txn)? {
            ResultSet::Scan { columns, mut rows } => {
                // 找到 order by 中列在表中的位置
                let mut order_col_index = HashMap::new();
                for (i, (col_name, _)) in self.order_by.iter().enumerate() {
                    if let Some(index) = columns.iter().position(|c| *c == *col_name) {
                        order_col_index.insert(i, index);
                    } else {
                        return Err(Error::Internal(format!(
                            "column {} not exist in table",
                            col_name
                        )));
                    }
                }

                // 对 order by 中的列进行排序
                rows.sort_by(|row1, row2| {
                    for (i, (_, ord)) in self.order_by.iter().enumerate() {
                        let col_indedx = order_col_index.get(&i).unwrap();
                        let a = &row1[*col_indedx];
                        let b = &row2[*col_indedx];
                        match a.partial_cmp(b) {
                            Some(Ordering::Equal) => {}
                            Some(o) => {
                                return if *ord == OrderDirection::Asc {
                                    o
                                } else {
                                    o.reverse()
                                }
                            }
                            None => {}
                        }
                    }

                    Ordering::Equal
                });
                Ok(ResultSet::Scan { columns, rows })
            }
            _ => return Err(Error::Internal("Unexpected result set".into())),
        }
    }
}
