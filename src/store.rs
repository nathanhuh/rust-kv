use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;

const TOMBSTONE: u8 = 1;
const LIVE: u8 = 0;

pub struct KvStore {
    append_file: File,
    read_file: File,
}

impl KvStore {
    pub fn open(path: &Path) -> Result<KvStore, Error> {
        let append = OpenOptions::new().append(true).create(true).open(path)?;
        let read = OpenOptions::new().read(true).open(path)?;

        let new_kv = KvStore {
            append_file: append,
            read_file: read,
        };

        Ok(new_kv)
    }

    pub fn set(&mut self, key: String, value: String) -> Result<(), Error> {
        let key_length = u32::try_from(key.len())
            .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?
            .to_be_bytes();
        let val_length = u32::try_from(value.len())
            .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?
            .to_be_bytes();
        let tombstone = LIVE.to_be_bytes();
        let key_bytes = key.as_bytes();
        let val_bytes = value.as_bytes();

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&key_length);
        buffer.extend_from_slice(&val_length);
        buffer.extend_from_slice(&tombstone);
        buffer.extend_from_slice(key_bytes);
        buffer.extend_from_slice(val_bytes);

        self.append_file.write_all(&buffer)?;

        Ok(())
    }

    pub fn get(&mut self, key: String) -> Result<Option<String>, Error> {
        self.read_file.seek(SeekFrom::Start(0))?;

        let mut last_value = None;
        loop {
            let mut buffer = [0; 4];
            match self.read_file.read_exact(&mut buffer) {
                Ok(_) => (),
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
            let key_len = u32::from_be_bytes(buffer);

            self.read_file.read_exact(&mut buffer)?;
            let val_len = u32::from_be_bytes(buffer);

            let mut tombstone_buffer = [0; 1];
            self.read_file.read_exact(&mut tombstone_buffer)?;
            let tombstone = u8::from_be_bytes(tombstone_buffer);

            let mut key_buffer = vec![0u8; key_len as usize];
            self.read_file.read_exact(&mut key_buffer)?;
            let entry_key =
                String::from_utf8(key_buffer).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            let mut entry_val = String::from("");
            if val_len != 0 {
                let mut val_buffer = vec![0u8; val_len as usize];
                self.read_file.read_exact(&mut val_buffer)?;
                entry_val = String::from_utf8(val_buffer)
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            }
            if entry_key == key {
                if tombstone == TOMBSTONE {
                    last_value = None;
                } else if tombstone == LIVE {
                    last_value = Some(entry_val);
                } else {
                    return Err(Error::new(ErrorKind::InvalidData, "invalid tombstone flag"));
                }
            }
        }

        Ok(last_value)
    }

    pub fn delete(&mut self, key: String) -> Result<(), Error> {
        let key_length = u32::try_from(key.len())
            .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?
            .to_be_bytes();
        let val_length = 0_u32.to_be_bytes();
        let tombstone = TOMBSTONE.to_be_bytes();
        let key_bytes = key.as_bytes();

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&key_length);
        buffer.extend_from_slice(&val_length);
        buffer.extend_from_slice(&tombstone);
        buffer.extend_from_slice(key_bytes);

        self.append_file.write_all(&buffer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(name: &str) {
        let _ = std::fs::remove_file(tmp_path(name));
    }

    #[test]
    fn set_then_get_returns_value() {
        let path = tmp_path("test_set_get.db");
        cleanup("test_set_get.db");
        let mut store = KvStore::open(&path).unwrap();
        store.set("foo".to_string(), "bar".to_string()).unwrap();
        assert_eq!(
            store.get("foo".to_string()).unwrap(),
            Some("bar".to_string())
        );
        cleanup("test_set_get.db");
    }

    #[test]
    fn get_missing_key_returns_none() {
        let path = tmp_path("test_missing.db");
        cleanup("test_missing.db");
        let mut store = KvStore::open(&path).unwrap();
        assert_eq!(store.get("missing".to_string()).unwrap(), None);
        cleanup("test_missing.db");
    }

    #[test]
    fn set_twice_returns_latest() {
        let path = tmp_path("test_overwrite.db");
        cleanup("test_overwrite.db");
        let mut store = KvStore::open(&path).unwrap();
        store.set("foo".to_string(), "first".to_string()).unwrap();
        store.set("foo".to_string(), "second".to_string()).unwrap();
        assert_eq!(
            store.get("foo".to_string()).unwrap(),
            Some("second".to_string())
        );
        cleanup("test_overwrite.db");
    }

    #[test]
    fn delete_returns_none() {
        let path = tmp_path("test_delete.db");
        cleanup("test_delete.db");
        let mut store = KvStore::open(&path).unwrap();
        store.set("foo".to_string(), "bar".to_string()).unwrap();
        store.delete("foo".to_string()).unwrap();
        assert_eq!(store.get("foo".to_string()).unwrap(), None);
        cleanup("test_delete.db");
    }

    #[test]
    fn delete_then_set_returns_new_value() {
        let path = tmp_path("test_delete_set.db");
        cleanup("test_delete_set.db");
        let mut store = KvStore::open(&path).unwrap();
        store.set("foo".to_string(), "bar".to_string()).unwrap();
        store.delete("foo".to_string()).unwrap();
        store.set("foo".to_string(), "baz".to_string()).unwrap();
        assert_eq!(
            store.get("foo".to_string()).unwrap(),
            Some("baz".to_string())
        );
        cleanup("test_delete_set.db");
    }

    #[test]
    fn data_persists_across_reopen() {
        let path = tmp_path("test_persist.db");
        cleanup("test_persist.db");
        {
            let mut store = KvStore::open(&path).unwrap();
            store.set("foo".to_string(), "bar".to_string()).unwrap();
        }
        let mut store2 = KvStore::open(&path).unwrap();
        assert_eq!(
            store2.get("foo".to_string()).unwrap(),
            Some("bar".to_string())
        );
        cleanup("test_persist.db");
    }
}
