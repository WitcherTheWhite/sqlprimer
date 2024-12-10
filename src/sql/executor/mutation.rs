use std::collections::HashMap;

use crate::{
    error::{Error, Result},
    sql::{
        engine::Transaction,
        parser::ast::Expression,
        schema::Table,
        types::{Row, Value},
    },
};

use super::{Executor, ResultSet};

pub struct Insert {
    table_name: String,
    columns: Vec<String>,
    values: Vec<Vec<Expression>>,
}

impl Insert {
    pub fn new(
        table_name: String,
        columns: Vec<String>,
        values: Vec<Vec<Expression>>,
    ) -> Box<Self> {
        Box::new(Self {
            table_name,
            columns,
            values,
        })
    }
}

// 列对齐，如果剩余列没有默认值则报错
fn pad_row(table: &Table, row: &Row) -> Result<Row> {
    let mut results = row.clone();
    for columm in table.columns.iter().skip(row.len()) {
        if let Some(default) = &columm.default {
            results.push(default.clone());
        } else {
            return Err(Error::Internal(format!(
                "No default value for column {}",
                columm.name
            )));
        }
    }

    Ok(results)
}

// 处理要插入的列
fn make_row(table: &Table, columns: &Vec<String>, values: &Row) -> Result<Row> {
    if columns.len() != values.len() {
        return Err(Error::Internal(format!("columns and values num mismatch")));
    }

    let mut inputs = HashMap::new();
    for (i, col_name) in columns.iter().enumerate() {
        inputs.insert(col_name, values[i].clone());
    }

    let mut results = Vec::new();
    for column in &table.columns {
        if let Some(value) = inputs.get(&column.name) {
            results.push(value.clone());
        } else if let Some(value) = &column.default {
            results.push(value.clone());
        } else {
            return Err(Error::Internal(format!(
                "No given value for column {}",
                column.name
            )));
        }
    }

    Ok(results)
}

impl<T: Transaction> Executor<T> for Insert {
    fn execute(self: Box<Self>, txn: &mut T) -> Result<ResultSet> {
        let mut count = 0;
        let table = txn.must_get_table(self.table_name.clone())?;
        for exprs in self.values {
            let row = exprs
                .into_iter()
                .map(|e| Value::from_expression(e))
                .collect::<Vec<_>>();
            let insert_row = if self.columns.is_empty() {
                pad_row(&table, &row)?
            } else {
                make_row(&table, &self.columns, &row)?
            };

            txn.create_row(self.table_name.clone(), insert_row)?;
            count += 1;
        }

        Ok(ResultSet::Insert { count })
    }
}
