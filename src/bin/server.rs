use futures::SinkExt;
use sqldb::{
    error::Result,
    sql::{self, engine::kv::KVEngine},
    storage::disk::DiskEngine,
};
use std::{
    env,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;
use tokio_util::codec::{Framed, LinesCodec};

const DB_PATH: &str = "/tmp/sqldb-test/sqldb-log";

/// Possible requests our clients can send us
enum SqlRequest {
    Sql(String),
    ListTable,
    TableInfo(String),
}

pub struct ServerSession<E: sql::engine::Engine> {
    session: sql::engine::Session<E>,
}

impl<E: sql::engine::Engine + 'static> ServerSession<E> {
    pub fn new(eng: MutexGuard<E>) -> Result<Self> {
        Ok(Self {
            session: eng.session()?,
        })
    }

    pub async fn handle_request(&mut self, socket: TcpStream) -> Result<()> {
        let mut lines = Framed::new(socket, LinesCodec::new());
        while let Some(result) = lines.next().await {
            match result {
                Ok(line) => {
                    let req = SqlRequest::Sql(line);

                    let resp = match req {
                        SqlRequest::Sql(sql) => self.session.execute(&sql),
                        SqlRequest::ListTable => todo!(),
                        SqlRequest::TableInfo(_) => todo!(),
                    };

                    // 发送执行结果
                    let response = match resp {
                        Ok(rs) => rs.to_string(),
                        Err(e) => e.to_string(),
                    };
                    if let Err(e) = lines.send(response.as_str()).await {
                        println!("error on sending response; error = {e:?}");
                    }
                }
                Err(e) => {
                    println!("error on decoding from socket; error = {e:?}");
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 启动 TCP 服务
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let listener = TcpListener::bind(&addr).await?;
    println!("sqldb server start, listening on: {addr}");

    // 初始化数据库
    let p = PathBuf::from(DB_PATH);
    let kvengine = KVEngine::new(DiskEngine::new(p.clone())?);
    let share_engine = Arc::new(Mutex::new(kvengine));

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                let db = share_engine.clone();
                let mut ss = ServerSession::new(db.lock()?)?;
                tokio::spawn(async move {
                    match ss.handle_request(socket).await {
                        Ok(_) => todo!(),
                        Err(_) => todo!(),
                    }
                });
            }
            Err(e) => println!("error accepting socket; error = {e:?}"),
        }
    }
}
