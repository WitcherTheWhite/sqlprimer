use std::{collections::BTreeMap, iter::Peekable};

use ast::Expression;
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
        self.next_expect(Token::Keyword(Keyword::Select))?;
        self.next_expect(Token::Asterisk)?;
        self.next_expect(Token::Keyword(Keyword::From))?;

        let table_name = self.next_ident()?;
        Ok(ast::Statement::Select { table_name })
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

    fn parse_where_clause(&mut self) -> Result<Option<(String, Expression)>> {
        if self.next_if_token(Token::Keyword(Keyword::Where)).is_none() {
            return Ok(None);
        }

        let col = self.next_ident()?;
        self.next_expect(Token::Equal)?;
        let value = self.parse_expression()?;

        Ok(Some((col, value)))
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
    use crate::{error::Result, sql::parser::ast};

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
        let sql = "select * from tbl1;";
        let stmt = Parser::new(sql).parse()?;
        assert_eq!(
            stmt,
            ast::Statement::Select {
                table_name: "tbl1".to_string()
            }
        );
        Ok(())
    }

    #[test]
    fn test_parser_update() -> Result<()> {
        let sql = "update tabl set a = 1, b = 2.0 where c = 'a';";
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
                where_clause: Some(("c".into(), ast::Consts::String("a".into()).into())),
            },
        );

        Ok(())
    }
}
