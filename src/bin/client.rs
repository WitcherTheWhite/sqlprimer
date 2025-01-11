use futures::{SinkExt, TryStreamExt};
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use std::env;
use std::error::Error;
use std::net::SocketAddr;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

const RESPONSE_END: &str = "!!!end!!!";

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub async fn new(addr: SocketAddr) -> Result<Self, Box<dyn Error>> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self { stream })
    }

    pub async fn execute_sql(&mut self, sql_cmd: &str) -> Result<(), Box<dyn Error>> {
        let (r, w) = self.stream.split();
        let mut sink = FramedWrite::new(w, LinesCodec::new());
        let mut stream = FramedRead::new(r, LinesCodec::new());

        // 发送命令并执行
        sink.send(sql_cmd).await?;

        // 拿到结果并打印
        while let Some(val) = stream.try_next().await? {
            if val == RESPONSE_END {
                break;
            }
            println!("{val}");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse what address we're going to connect to
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let addr = addr.parse::<SocketAddr>()?;
    let mut client = Client::new(addr).await?;

    let mut editor = DefaultEditor::new()?;
    loop {
        let readline = editor.readline("sqldb>> ");
        match readline {
            Ok(sql_cmd) => {
                let sql_cmd = sql_cmd.trim();
                if sql_cmd.len() > 0 {
                    if sql_cmd == "quit" {
                        break;
                    }
                    editor.add_history_entry(sql_cmd)?;
                    client.execute_sql(sql_cmd).await?;
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
