use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use fs4::fs_std::FileExt;

use crate::error::Result;

use super::engine::{Engine, EngineIterator};

pub type KeyDir = BTreeMap<Vec<u8>, (u64, u32)>;

const LOG_HEADER_SIZE: u32 = 8;

pub struct DiskEngine {
    keydir: KeyDir,
    log: Log,
}

impl DiskEngine {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let mut log = Log::new(file_path)?;
        let keydir = log.build_keydir()?;
        Ok(Self { keydir, log })
    }

    pub fn new_compact(file_path: PathBuf) -> Result<Self> {
        let mut eng = Self::new(file_path)?;
        eng.compact()?;
        Ok(eng)
    }

    fn compact(&mut self) -> Result<()> {
        // 打开一个临时文件
        let mut new_path = self.log.file_path.clone();
        new_path.set_extension("compact");

        let mut new_keydir = KeyDir::new();
        let mut new_log = Log::new(new_path)?;

        // 遍历索引写入到新的数据文件
        for (key, (offset, val_size)) in self.keydir.iter() {
            let value = self.log.read_value(*offset, *val_size)?;
            let (new_offset, new_size) = new_log.write_entry(key, Some(&value))?;
            new_keydir.insert(
                key.to_vec(),
                (new_offset + new_size as u64 - *val_size as u64, new_size),
            );
        }

        std::fs::rename(&new_log.file_path, &self.log.file_path)?;

        new_log.file_path = self.log.file_path.clone();
        self.keydir = new_keydir;
        self.log = new_log;

        Ok(())
    }
}

impl Engine for DiskEngine {
    type EngineIterator<'a> = DiskEngineIterator;

    fn set(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let (offset, size) = self.log.write_entry(&key, Some(&value))?;
        let val_size = value.len() as u32;
        self.keydir
            .insert(key, (offset + size as u64 - val_size as u64, val_size));
        Ok(())
    }

    fn get(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        if let Some((offset, val_size)) = self.keydir.get(&key) {
            let value = self.log.read_value(*offset, *val_size)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: Vec<u8>) -> Result<()> {
        self.log.write_entry(&key, None)?;
        self.keydir.remove(&key);
        Ok(())
    }

    fn scan<'a>(
        &mut self,
        _range: impl std::ops::RangeBounds<Vec<u8>>,
    ) -> Self::EngineIterator<'_> {
        todo!()
    }
}

pub struct DiskEngineIterator {}

impl EngineIterator for DiskEngineIterator {}

impl Iterator for DiskEngineIterator {
    type Item = Result<(Vec<u8>, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl DoubleEndedIterator for DiskEngineIterator {
    fn next_back(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

pub struct Log {
    file: std::fs::File,
    file_path: PathBuf,
}

impl Log {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        if let Some(dir) = file_path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir)?;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(file_path.clone())?;

        // 加文件锁
        file.try_lock_exclusive()?;

        Ok(Self { file, file_path })
    }

    // 遍历数据文件，构建内存索引
    fn build_keydir(&mut self) -> Result<KeyDir> {
        let mut keydir = KeyDir::new();
        let file_size = self.file.metadata()?.len();
        let mut buf_reader = BufReader::new(&self.file);

        let mut offset = 0;
        loop {
            if offset >= file_size {
                break;
            }

            let (key, val_size) = self.read_entry(&mut buf_reader, offset)?;
            let key_size = key.len() as u64;
            // 墓碑值，在内存索引中删除
            if val_size == -1 {
                keydir.remove(&key);
                offset += key_size + LOG_HEADER_SIZE as u64;
            } else {
                keydir.insert(
                    key,
                    (offset + LOG_HEADER_SIZE as u64 + key_size, val_size as u32),
                );
                offset += key_size + LOG_HEADER_SIZE as u64 + val_size as u64;
            }
        }

        Ok(keydir)
    }

    fn write_entry(&mut self, key: &Vec<u8>, value: Option<&Vec<u8>>) -> Result<(u64, u32)> {
        let offset = self.file.seek(SeekFrom::End(0))?;
        let key_size = key.len() as u32;
        let val_size = value.map_or(0, |v| v.len() as u32);
        let total_size = key_size + val_size + LOG_HEADER_SIZE;

        // 写入 key 长度、value 长度、key/value 实际的值
        let mut writer = BufWriter::with_capacity(total_size as usize, &self.file);
        writer.write_all(&key_size.to_be_bytes())?;
        writer.write_all(&value.map_or(-1, |v| v.len() as i32).to_be_bytes())?;
        writer.write_all(&key)?;
        if let Some(v) = value {
            writer.write_all(v)?;
        }
        writer.flush()?;

        Ok((offset, val_size))
    }

    fn read_value(&mut self, offset: u64, val_size: u32) -> Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0; val_size as usize];
        self.file.read_exact(&mut buf)?;

        Ok(buf)
    }

    fn read_entry(&self, buf_reader: &mut BufReader<&File>, offset: u64) -> Result<(Vec<u8>, i32)> {
        buf_reader.seek(SeekFrom::Start(offset))?;
        let mut len_buf = [0; 4];

        buf_reader.read_exact(&mut len_buf)?;
        let key_size = u32::from_be_bytes(len_buf);

        buf_reader.read_exact(&mut len_buf)?;
        let val_size = i32::from_be_bytes(len_buf);

        let mut key = vec![0; key_size as usize];
        buf_reader.read_exact(&mut key)?;

        Ok((key, val_size))
    }
}

#[test]
fn test_disk_engine_start() -> Result<()> {
    let eng = DiskEngine::new_compact(PathBuf::from("/tmp/sqldb-log"))?;
    Ok(())
}
