use std::{collections::BTreeMap, iter::Peekable};

use ast::{Expression, FromItem, Operation, OrderDirection};
use lexer::{Keyword, Lexer, Token};

use crate::error::{Error, Result};

use super::types::DataType;

pub mod ast;
mod lexer;

// 解析器定义
pub struct Parser<'a> {
    lexer: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            lexer: Lexer::new(input).peekable(),
        }
    }

    // 解析得到抽象语法树
    pub fn parse(&mut self) -> Result<ast::Statement> {
        let stmt = self.parse_statement()?;
        // sql 语句用分号结尾
        self.next_expect(Token::Semicolon)?;
        // 之后不能有其他字符
        if let Some(token) = self.peek()? {
            return Err(Error::Parse(format!("[Parser] Unexpected token {}", token)));
        }
        Ok(stmt)
    }

    fn parse_statement(&mut self) -> Result<ast::Statement> {
        match self.peek()? {
            Some(Token::Keyword(Keyword::Create)) => self.parse_ddl(),
            Some(Token::Keyword(Keyword::Select)) => self.parse_select(),
            Some(Token::Keyword(Keyword::Insert)) => self.parse_insert(),
            Some(Token::Keyword(Keyword::Update)) => self.parse_update(),
            Some(Token::Keyword(Keyword::Delete)) => self.parse_delete(),
            Some(t) => Err(Error::Parse(format!("[Parser] Unexpected token {}", t))),
            None => Err(Error::Parse(format!("[Parser] Unexpected end of input"))),
        }
    }

    fn parse_ddl(&mut self) -> Result<ast::Statement> {
        match self.next()? {
            Token::Keyword(Keyword::Create) => match self.next()? {
                Token::Keyword(Keyword::Table) => self.parse_ddl_create_table(),
                token => Err(Error::Parse(format!("[Parser] Unexpected token {}", token))),
            },
            token => Err(Error::Parse(format!("[Parser] Unexpected token {}", token))),
        }
    }

    fn parse_select(&mut self) -> Result<ast::Statement> {
        let select = self.parse_select_clause()?;

        Ok(ast::Statement::Select {
            select,
            from: self.parse_from_clause()?,
            where_clause: self.parse_where_clause()?,
            group_by: self.parse_group_clause()?,
            having: self.parse_having_clause()?,
            order_by: self.parse_order_clause()?,
            limit: {
                if self.next_if_token(Token::Keyword(Keyword::Limit)).is_some() {
                    Some(self.parse_expression()?)
                } else {
                    None
                }
            },
            offset: {
                if self
                    .next_if_token(Token::Keyword(Keyword::Offset))
                    .is_some()
                {
                    Some(self.parse_expression()?)
                } else {
                    None
                }
            },
        })
    }

    fn parse_group_clause(&mut self) -> Result<Option<Expression>> {
        if self.next_if_token(Token::Keyword(Keyword::Group)).is_none() {
            return Ok(None);
        }
        self.next_expect(Token::Keyword(Keyword::By))?;

        Ok(Some(self.parse_expression()?))
    }

    fn parse_having_clause(&mut self) -> Result<Option<Expression>> {
        if self
            .next_if_token(Token::Keyword(Keyword::Having))
            .is_none()
        {
            return Ok(None);
        }

        Ok(Some(self.parse_operation_expr()?))
    }

    fn parse_from_clause(&mut self) -> Result<ast::FromItem> {
        self.next_expect(Token::Keyword(Keyword::From))?;

        let mut item: FromItem = self.parse_from_table_clause()?;
        while let Some(join_type) = self.parse_from_clause_join()? {
            let left = Box::new(item);
            let right = Box::new(self.parse_from_table_clause()?);

            // 解析 join 条件
            let predicate = match join_type {
                ast::JoinType::Cross => None,
                _ => {
                    self.next_expect(Token::Keyword(Keyword::On))?;
                    let l = self.parse_expression()?;
                    self.next_expect(Token::Equal)?;
                    let r = self.parse_expression()?;

                    let (l, r) = match join_type {
                        ast::JoinType::Right => (r, l),
                        _ => (l, r),
                    };

                    Some(Expression::Operation(Operation::Equal(
                        Box::new(l),
                        Box::new(r),
                    )))
                }
            };

            item = FromItem::Join {
                left,
                right,
                join_type,
                predicate,
            }
        }

        Ok(item)
    }

    fn parse_from_table_clause(&mut self) -> Result<ast::FromItem> {
        Ok(ast::FromItem::Table {
            name: self.next_ident()?,
        })
    }

    fn parse_from_clause_join(&mut self) -> Result<Option<ast::JoinType>> {
        if self.next_if_token(Token::Keyword(Keyword::Cross)).is_some() {
            self.next_expect(Token::Keyword(Keyword::Join))?;
            return Ok(Some(ast::JoinType::Cross));
        } else if self.next_if_token(Token::Keyword(Keyword::Join)).is_some() {
            return Ok(Some(ast::JoinType::Inner));
        } else if self.next_if_token(Token::Keyword(Keyword::Left)).is_some() {
            self.next_expect(Token::Keyword(Keyword::Join))?;
            return Ok(Some(ast::JoinType::Left));
        } else if self.next_if_token(Token::Keyword(Keyword::Right)).is_some() {
            self.next_expect(Token::Keyword(Keyword::Join))?;
            return Ok(Some(ast::JoinType::Right));
        }

        Ok(None)
    }

    fn parse_operation_expr(&mut self) -> Result<ast::Expression> {
        let left = self.parse_expression()?;
        Ok(match self.next()? {
            Token::Equal => Expression::Operation(Operation::Equal(
                Box::new(left),
                Box::new(self.parse_expression()?),
            )),
            Token::GreaterThan => Expression::Operation(Operation::GreaterThan(
                Box::new(left),
                Box::new(self.parse_expression()?),
            )),
            Token::LessThan => Expression::Operation(Operation::LessThan(
                Box::new(left),
                Box::new(self.parse_expression()?),
            )),
            _ => return Err(Error::Internal("unexpected token".into())),
        })
    }

    fn parse_insert(&mut self) -> Result<ast::Statement> {
        self.next_expect(Token::Keyword(Keyword::Insert))?;
        self.next_expect(Token::Keyword(Keyword::Into))?;

        let table_name = self.next_ident()?;

        // 要插入的列名，可选
        let columns = if self.next_if_token(Token::OpenParen).is_some() {
            let mut cols = Vec::new();
            loop {
                cols.push(self.next_ident()?);
                match self.next()? {
                    Token::CloseParen => break,
                    Token::Comma => {}
                    token => {
                        return Err(Error::Parse(format!(
                            "[Parser] Unexpected keyword {}",
                            token
                        )))
                    }
                }
            }
            Some(cols)
        } else {
            None
        };

        // 要插入的数据，可以有多行
        self.next_expect(Token::Keyword(Keyword::Values))?;
        let mut values = Vec::new();
        loop {
            self.next_expect(Token::OpenParen)?;
            let mut exprs = Vec::new();
            loop {
                exprs.push(self.parse_expression()?);
                match self.next()? {
                    Token::CloseParen => break,
                    Token::Comma => {}
                    token => {
                        return Err(Error::Parse(format!(
                            "[Parser] Unexpected keyword {}",
                            token
                        )))
                    }
                }
            }
            values.push(exprs);
            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }

        Ok(ast::Statement::Insert {
            table_name,
            columns,
            values,
        })
    }

    fn parse_update(&mut self) -> Result<ast::Statement> {
        self.next_expect(Token::Keyword(Keyword::Update))?;
        let table_name = self.next_ident()?;
        self.next_expect(Token::Keyword(Keyword::Set))?;

        // 要更新的列和数据
        let mut columns = BTreeMap::new();
        loop {
            let col = self.next_ident()?;
            if columns.contains_key(&col) {
                return Err(Error::Parse(format!(
                    "[Parser] Duplicate column {} for update",
                    col
                )));
            }
            self.next_expect(Token::Equal)?;
            let value = self.parse_expression()?;
            columns.insert(col, value);
            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }

        Ok(ast::Statement::Update {
            table_name,
            columns,
            where_clause: self.parse_where_clause()?,
        })
    }

    fn parse_delete(&mut self) -> Result<ast::Statement> {
        self.next_expect(Token::Keyword(Keyword::Delete))?;
        self.next_expect(Token::Keyword(Keyword::From))?;
        let table_name = self.next_ident()?;

        Ok(ast::Statement::Delete {
            table_name,
            where_clause: self.parse_where_clause()?,
        })
    }

    fn parse_where_clause(&mut self) -> Result<Option<Expression>> {
        if self.next_if_token(Token::Keyword(Keyword::Where)).is_none() {
            return Ok(None);
        }

        Ok(Some(self.parse_operation_expr()?))
    }

    fn parse_order_clause(&mut self) -> Result<Vec<(String, OrderDirection)>> {
        let mut orders = Vec::new();
        if self.next_if_token(Token::Keyword(Keyword::Order)).is_none() {
            return Ok(orders);
        }
        self.next_expect(Token::Keyword(Keyword::By))?;

        loop {
            let col = self.next_ident()?;
            let ord = match self.next_if(|t| {
                matches!(
                    t,
                    Token::Keyword(Keyword::Asc) | Token::Keyword(Keyword::Desc)
                )
            }) {
                Some(Token::Keyword(Keyword::Asc)) => OrderDirection::Asc,
                Some(Token::Keyword(Keyword::Desc)) => OrderDirection::Desc,
                _ => OrderDirection::Asc,
            };
            orders.push((col, ord));

            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }

        Ok(orders)
    }

    fn parse_select_clause(&mut self) -> Result<Vec<(Expression, Option<String>)>> {
        self.next_expect(Token::Keyword(Keyword::Select))?;

        let mut select = Vec::new();
        if self.next_if_token(Token::Asterisk).is_some() {
            return Ok(select);
        }

        loop {
            let expr = self.parse_expression()?;
            let alias = match self.next_if_token(Token::Keyword(Keyword::As)) {
                Some(_) => Some(self.next_ident()?),
                None => None,
            };
            select.push((expr, alias));

            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }

        Ok(select)
    }

    fn parse_ddl_create_table(&mut self) -> Result<ast::Statement> {
        let table_name = self.next_ident()?;
        self.next_expect(Token::OpenParen)?;

        let mut columns = Vec::new();
        loop {
            columns.push(self.parse_ddl_column()?);
            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }
        self.next_expect(Token::CloseParen)?;

        Ok(ast::Statement::CreateTable {
            name: table_name,
            columns,
        })
    }

    fn parse_ddl_column(&mut self) -> Result<ast::Column> {
        let mut column = ast::Column {
            name: self.next_ident()?,
            datatype: self.next_datatype()?,
            nullable: None,
            default: None,
            primary_key: false,
        };

        // 解析是否为空和默认值
        while let Some(Token::Keyword(keyword)) = self.next_if_keyword() {
            match keyword {
                Keyword::Null => column.nullable = Some(true),
                Keyword::Not => {
                    self.next_expect(Token::Keyword(Keyword::Null))?;
                    column.nullable = Some(false);
                }
                Keyword::Default => column.default = Some(self.parse_expression()?),
                Keyword::Primary => {
                    self.next_expect(Token::Keyword(Keyword::Key))?;
                    column.primary_key = true;
                }
                k => return Err(Error::Parse(format!("[Parser] Unexpected keyword {}", k))),
            }
        }

        Ok(column)
    }

    fn parse_expression(&mut self) -> Result<ast::Expression> {
        Ok(match self.next()? {
            Token::Ident(ident) => {
                // 函数名称 eg.max(col_name)
                if self.next_if_token(Token::OpenParen).is_some() {
                    let col_name = self.next_ident()?;
                    self.next_expect(Token::CloseParen)?;
                    ast::Expression::Function(ident, col_name)
                } else {
                    ast::Expression::Filed(ident)
                }
            }
            Token::Number(n) => {
                if n.chars().all(|c| c.is_ascii_digit()) {
                    // 整数
                    ast::Consts::Integer(n.parse()?).into()
                } else {
                    // 浮点数
                    ast::Consts::Float(n.parse()?).into()
                }
            }
            Token::String(s) => ast::Consts::String(s).into(),
            Token::Keyword(Keyword::True) => ast::Consts::Boolean(true).into(),
            Token::Keyword(Keyword::False) => ast::Consts::Boolean(false).into(),
            Token::Keyword(Keyword::Null) => ast::Consts::Null.into(),
            t => {
                return Err(Error::Parse(format!(
                    "[Parser] Unexpected expression token {}",
                    t
                )))
            }
        })
    }

    fn peek(&mut self) -> Result<Option<Token>> {
        self.lexer.peek().cloned().transpose()
    }

    fn next(&mut self) -> Result<Token> {
        self.lexer
            .next()
            .unwrap_or_else(|| Err(Error::Parse(format!("[Parser] Unexpected end of input"))))
    }

    fn next_ident(&mut self) -> Result<String> {
        match self.next()? {
            Token::Ident(ident) => Ok(ident),
            token => Err(Error::Parse(format!(
                "[Parser] Expected ident, got token {}",
                token
            ))),
        }
    }

    fn next_datatype(&mut self) -> Result<DataType> {
        match self.next()? {
            Token::Keyword(Keyword::Bool) | Token::Keyword(Keyword::Boolean) => {
                Ok(DataType::Boolean)
            }
            Token::Keyword(Keyword::Int) | Token::Keyword(Keyword::Integer) => {
                Ok(DataType::Integer)
            }
            Token::Keyword(Keyword::Float) | Token::Keyword(Keyword::Double) => Ok(DataType::Float),
            Token::Keyword(Keyword::String)
            | Token::Keyword(Keyword::Text)
            | Token::Keyword(Keyword::Varchar) => Ok(DataType::String),
            token => Err(Error::Parse(format!(
                "[Parser] Expected datatype, got token {}",
                token
            ))),
        }
    }

    fn next_expect(&mut self, expect: Token) -> Result<()> {
        let token = self.next()?;
        if token != expect {
            return Err(Error::Parse(format!(
                "[Parser] Expected token {}, got {}",
                expect, token
            )));
        }

        Ok(())
    }

    // 如果满足条件，跳转到下一个 Token
    fn next_if<F: Fn(&Token) -> bool>(&mut self, predicate: F) -> Option<Token> {
        self.peek().unwrap_or(None).filter(|t| predicate(t))?;
        self.next().ok()
    }

    // 如果下一个 Token 是关键字，则跳转
    fn next_if_keyword(&mut self) -> Option<Token> {
        self.next_if(|t| matches!(t, Token::Keyword(_)))
    }

    fn next_if_token(&mut self, token: Token) -> Option<Token> {
        self.next_if(|t| *t == token)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::Result,
        sql::parser::ast::{
            self, Consts, Expression, FromItem, JoinType, Operation, OrderDirection,
        },
    };

    use super::Parser;

    #[test]
    fn test_parser_create_table() -> Result<()> {
        let sql1 = "
            create table tbl1 (
                a int default 100,
                b float not null,
                c varchar null,
                d bool default true
            );
        ";
        let stmt1 = Parser::new(sql1).parse()?;

        let sql2 = "
        create            table tbl1 (
            a int default     100,
            b float not null     ,
            c varchar      null,
            d       bool default        true
        );
        ";
        let stmt2 = Parser::new(sql2).parse()?;
        assert_eq!(stmt1, stmt2);

        let sql3 = "
            create            table tbl1 (
            a int default     100,
            b float not null     ,
            c varchar      null,
            d       bool default        true
        )
        ";

        let stmt3 = Parser::new(sql3).parse();
        assert!(stmt3.is_err());
        Ok(())
    }

    #[test]
    fn test_parser_insert() -> Result<()> {
        let sql1 = "insert into tbl1 values (1, 2, 3, 'a', true);";
        let stmt1 = Parser::new(sql1).parse()?;
        assert_eq!(
            stmt1,
            ast::Statement::Insert {
                table_name: "tbl1".to_string(),
                columns: None,
                values: vec![vec![
                    ast::Consts::Integer(1).into(),
                    ast::Consts::Integer(2).into(),
                    ast::Consts::Integer(3).into(),
                    ast::Consts::String("a".to_string()).into(),
                    ast::Consts::Boolean(true).into(),
                ]],
            }
        );

        let sql2 = "insert into tbl2 (c1, c2, c3) values (3, 'a', true),(4, 'b', false);";
        let stmt2 = Parser::new(sql2).parse()?;
        assert_eq!(
            stmt2,
            ast::Statement::Insert {
                table_name: "tbl2".to_string(),
                columns: Some(vec!["c1".to_string(), "c2".to_string(), "c3".to_string()]),
                values: vec![
                    vec![
                        ast::Consts::Integer(3).into(),
                        ast::Consts::String("a".to_string()).into(),
                        ast::Consts::Boolean(true).into(),
                    ],
                    vec![
                        ast::Consts::Integer(4).into(),
                        ast::Consts::String("b".to_string()).into(),
                        ast::Consts::Boolean(false).into(),
                    ],
                ],
            }
        );

        Ok(())
    }

    #[test]
    fn test_parser_select() -> Result<()> {
        let sql =
            "select a as hsy, b from tbl1 cross join tbl2 cross join tbl3 limit 10 offset 10;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Join {
                    left: Box::new(FromItem::Join {
                        left: Box::new(FromItem::Table {
                            name: "tbl1".to_string()
                        }),
                        right: Box::new(FromItem::Table {
                            name: "tbl2".to_string()
                        }),
                        join_type: JoinType::Cross,
                        predicate: None
                    }),
                    right: Box::new(FromItem::Table {
                        name: "tbl3".to_string()
                    }),
                    join_type: ast::JoinType::Cross,
                    predicate: None
                },
                order_by: vec![],
                limit: Some(Expression::Consts(Consts::Integer(10))),
                offset: Some(Expression::Consts(Consts::Integer(10))),
                select: vec![
                    (Expression::Filed("a".to_string()), Some("hsy".to_string())),
                    (Expression::Filed("b".to_string()), None),
                ],
                group_by: None,
                where_clause: None,
                having: None,
            }
        );

        let sql = "select * from tbl1 order by a, b asc, c desc limit 5 offset 20;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Table {
                    name: "tbl1".to_string()
                },
                order_by: vec![
                    ("a".to_string(), OrderDirection::Asc),
                    ("b".to_string(), OrderDirection::Asc),
                    ("c".to_string(), OrderDirection::Desc),
                ],
                limit: Some(Expression::Consts(Consts::Integer(5))),
                offset: Some(Expression::Consts(Consts::Integer(20))),
                select: vec![],
                group_by: None,
                where_clause: None,
                having: None,
            }
        );

        let sql = "select count(a), max(b), min(c) from tbl1;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Table {
                    name: "tbl1".to_string()
                },
                order_by: vec![],
                limit: None,
                offset: None,
                select: vec![
                    (Expression::Function("count".into(), "a".into()), None),
                    (Expression::Function("max".into(), "b".into()), None),
                    (Expression::Function("min".into(), "c".into()), None)
                ],
                group_by: None,
                where_clause: None,
                having: None,
            }
        );

        let sql = "select count(a), max(b), min(c) from tbl1 group by a;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Table {
                    name: "tbl1".to_string()
                },
                order_by: vec![],
                limit: None,
                offset: None,
                select: vec![
                    (Expression::Function("count".into(), "a".into()), None),
                    (Expression::Function("max".into(), "b".into()), None),
                    (Expression::Function("min".into(), "c".into()), None)
                ],
                group_by: Some(Expression::Filed("a".into())),
                where_clause: None,
                having: None,
            }
        );

        let sql = "select * from tbl1 where a > 100;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Table {
                    name: "tbl1".to_string()
                },
                order_by: vec![],
                limit: None,
                offset: None,
                select: vec![],
                group_by: None,
                where_clause: Some(Expression::Operation(Operation::GreaterThan(
                    Box::new(Expression::Filed("a".into())),
                    Box::new(Expression::Consts(Consts::Integer(100)))
                ))),
                having: None,
            }
        );

        let sql = "select * from tbl1 group by a having a > 100;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                from: FromItem::Table {
                    name: "tbl1".to_string()
                },
                order_by: vec![],
                limit: None,
                offset: None,
                select: vec![],
                group_by: Some(Expression::Filed("a".into())),
                where_clause: None,
                having: Some(Expression::Operation(Operation::GreaterThan(
                    Box::new(Expression::Filed("a".into())),
                    Box::new(Expression::Consts(Consts::Integer(100)))
                ))),
            }
        );

        Ok(())
    }

    #[test]
    fn test_parser_update() -> Result<()> {
        let sql = "update tabl set a = 1, b = 2.0 where c < 'a';";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Update {
                table_name: "tabl".into(),
                columns: vec![
                    ("a".into(), ast::Consts::Integer(1).into()),
                    ("b".into(), ast::Consts::Float(2.0).into()),
                ]
                .into_iter()
                .collect(),
                where_clause: Some(Expression::Operation(Operation::LessThan(
                    Box::new(Expression::Filed("c".into())),
                    Box::new(Expression::Consts(Consts::String("a".into())))
                ))),
            },
        );

        Ok(())
    }
}
