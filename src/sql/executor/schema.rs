use crate::{error::Result, sql::schema::Table};

use super::{Executor, ResultSet};

pub struct CreateTable {
    schema: Table,
}

impl CreateTable {
    pub fn new(schema: Table) -> Box<Self> {
        Box::new(Self { schema })
    }
}

impl Executor for CreateTable {
    fn execute(&self) -> Result<ResultSet> {
        todo!()
    }
}
