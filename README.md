# sqlprimer
Rust 实现的简易 SQL 数据库系统

## 安装使用

###  Rust环境安装
https://www.rust-lang.org/tools/

### 启动服务端
```bash
 cd sqlprimer/src/bin/
 cargo run --bin server
```

### 使用客服端命令行交互
```bash
 cargo run --bin client
```

### 基本操作
```bash
sqldb>> create table t1 (a int primary key, b text, c float);
CREATE TABLE t1
sqldb>> insert into t1 values (1, 'aa', 3.1), (2, 'cc', 5.3);
INSERT 2 rows
sqldb>> select * from t1;
a |b  |c  
--+---+----
1 |AA |3.1
2 |CC |5.3
(2 rows)
```

## 支持的 SQL 语法

### 1. Create/Drop Table
create table:
```sql
CREATE TABLE table_name (
    [ column_name data_type [index] [ column_constraint [...] ] ]
    [, ... ]
   );

   where data_type is:
    - BOOLEAN(BOOL): true | false
    - FLOAT(DOUBLE)
    - INTEGER(INT)
    - STRING(TEXT, VARCHAR)

   where column_constraint is:
   [ NOT NULL | NULL | DEFAULT expr ]
```
drop table:
```sql
DROP TABLE table_name;
```

### 2. Insert Into
```sql
INSERT INTO table_name
[ ( column_name [, ...] ) ]
values ( expr [, ...] );
```

### 3. Select
```sql
SELECT [* | col_name | function [ [ AS ] output_name [, ...] ]]
FROM from_item
[GROUP BY col_name]
[ORDER BY col_name [asc | desc] [, ...]]
[LIMIT count]
[OFFSET count]
```

where `function` is:
* count(col_name)
* min(col_name)
* max(col_name)
* sum(col_name)
* avg(col_name)

where `from_item` is:
* table_name
* table_name `join_type` table_name [`ON` predicate]

where `join_type` is:
* cross join
* join
* left join
* right join

where `on predicate` is:
* column_name = column_name

### 4. Update
```sql
UPDATE table_name
SET column_name = expr [, ...]
[WHERE condition];
```
where condition is: `column_name = expr`

### 5. Delete
```sql
DELETE FROM table_name
[WHERE condition];
```
where condition is: `column_name = expr`

### 5. Show Table
```sql
SHOW TABLES;
```

```sql
SHOW TABLE `table_name`;
```

### 6. Transaction

```
BEGIN;

COMMIT;

ROLLBACK;
```

## 7. Explain
```
explain sql;
```
## 参考资料
1. https://github.com/apache/datafusion-sqlparser-rs.git
2. https://github.com/apache/datafusion.git
3. https://github.com/postgres/postgres.git 
4. https://riak.com/assets/bitcask-intro.pdf
