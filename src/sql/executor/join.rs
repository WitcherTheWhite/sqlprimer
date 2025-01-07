use crate::{error::{Error, Result}, sql::engine::Transaction};

use super::{Executor, ResultSet};

pub struct NestedLoopJoin<T: Transaction> {
    left: Box<dyn Executor<T>>,
    right: Box<dyn Executor<T>>,
}

impl<T: Transaction> NestedLoopJoin<T> {
    pub fn new(left: Box<dyn Executor<T>>, right: Box<dyn Executor<T>>) -> Box<Self> {
        Box::new(Self { left, right })
    }
}

impl<T: Transaction> Executor<T> for NestedLoopJoin<T> {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        if let ResultSet::Scan { columns: lcols, rows: lrows } = self.left.execute(txn)? {
            let mut new_rows = Vec::new();
            let mut new_cols = lcols;
            if let ResultSet::Scan { columns: rcols, rows: rrows } = self.right.execute(txn)? {
                new_cols.extend(rcols);
                for lrow in &lrows {
                    for rrow in &rrows {
                        let mut row = lrow.clone();
                        row.extend(rrow.clone());
                        new_rows.push(row);
                    }
                }
                return Ok(ResultSet::Scan { columns: new_cols, rows: new_rows });
            }
        }

        Err(Error::Internal("Unexpected result set".into()))
    }
}
