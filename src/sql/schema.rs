use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::types::{DataType, Row, Value};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

impl Table {
    // 校验表的有效性
    pub fn validate(&self) -> Result<()> {
        // 校验是否有列信息
        if self.columns.is_empty() {
            return Err(Error::Internal(format!(
                "table {} has no columns",
                self.name
            )));
        }

        // 校验是否有主键
        match self.columns.iter().filter(|c| c.primary_key).count() {
            1 => {}
            0 => {
                return Err(Error::Internal(format!(
                    "No primary key for table {}",
                    self.name
                )))
            }
            _ => {
                return Err(Error::Internal(format!(
                    "Mutiple primary keys for table {}",
                    self.name
                )))
            }
        }

        // 校验列信息
        for col in &self.columns {
            // 主键不能为空
            if col.primary_key && col.nullable {
                return Err(Error::Internal(format!(
                    "Primary key {} cannot be nullable in table {}",
                    col.name, self.name,
                )));
            }
            // 默认值数据类型要和列类型匹配
            if let Some(v) = &col.default {
                match v.datatype() {
                    Some(dt) => {
                        if dt != col.datatype {
                            return Err(Error::Internal(format!(
                                "Default value for column {} mismatch in table {}",
                                col.name, self.name
                            )));
                        }
                    }
                    None => {}
                }
            }
        }

        Ok(())
    }

    pub fn get_primary_key(&self, row: &Row) -> Result<Value> {
        let pos = self
            .columns
            .iter()
            .position(|c| c.primary_key)
            .expect("No primary key found");
        Ok(row[pos].clone())
    }

    pub fn get_col_indedx(&self, col_name: &str) -> Result<usize> {
        self.columns
            .iter()
            .position(|c| c.name == col_name)
            .ok_or(Error::Internal(format!("Column {} not found", col_name)))
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub datatype: DataType,
    pub nullable: bool,
    pub default: Option<Value>,
    pub primary_key: bool,
}
