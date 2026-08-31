use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;

const PAGE_SIZE: usize = 4096;
const PAGE_ID_OFFSET: usize = 0;
const FREE_SPACE_OFFSET: usize = 4;
const HEADER_SIZE: usize = 6;
const NUM_PAGES_OFFSET: usize = HEADER_SIZE;

struct Page([u8; PAGE_SIZE]);
struct PageManager {
    file: File,
    num_pages: u32,
}

impl Page {
    fn read_u32(&self, offset: usize) -> u32 {
        let bytes = self.0[offset..offset + 4].try_into().unwrap();
        u32::from_be_bytes(bytes)
    }

    fn write_u32(&mut self, offset: usize, value: u32) {
        let bytes = value.to_be_bytes();
        self.0[offset..offset + 4].copy_from_slice(&bytes);
    }

    fn read_u16(&self, offset: usize) -> u16 {
        let bytes = self.0[offset..offset + 2].try_into().unwrap();
        u16::from_be_bytes(bytes)
    }

    fn write_u16(&mut self, offset: usize, value: u16) {
        let bytes = value.to_be_bytes();
        self.0[offset..offset + 2].copy_from_slice(&bytes);
    }
}

impl PageManager {
    fn open(path: &Path) -> Result<PageManager, Error> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let file_len = file.metadata()?.len();

        let num_pages = if file_len == 0 {
            let mut page_zero = Page([0; PAGE_SIZE]);
            page_zero.write_u32(PAGE_ID_OFFSET, 0);
            page_zero.write_u16(
                FREE_SPACE_OFFSET,
                u16::try_from(PAGE_SIZE - HEADER_SIZE - 4).unwrap(),
            );
            page_zero.write_u32(NUM_PAGES_OFFSET, 1);
            file.write_all(&page_zero.0)?;
            1
        } else {
            let mut page_zero = Page([0; PAGE_SIZE]);
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut page_zero.0)?;
            page_zero.read_u32(NUM_PAGES_OFFSET)
        };

        let intended_len = num_pages as u64 * PAGE_SIZE as u64;
        if file_len != 0 && file_len != intended_len {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "file length and page number does not match; invalid data",
            ));
        }

        Ok(PageManager { file, num_pages })
    }

    fn read_page(&mut self, page_id: u32) -> Result<Page, Error> {
        if page_id >= self.num_pages {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "page_id out of bounds; page does not exist",
            ));
        }

        let offset = page_id as u64 * PAGE_SIZE as u64;
        let mut page = Page([0; PAGE_SIZE]);

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut page.0)?;

        if page.read_u32(PAGE_ID_OFFSET) != page_id {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "page_id doesn't match; invalid data",
            ));
        }

        Ok(page)
    }

    fn write_page(&mut self, page_id: u32, page: Page) -> Result<(), Error> {
        if page_id >= self.num_pages {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "page_id out of bounds; page does not exist",
            ));
        }

        if page.read_u32(PAGE_ID_OFFSET) != page_id {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "page_id doesn't match; invalid data",
            ));
        }

        let offset = page_id as u64 * PAGE_SIZE as u64;

        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&page.0)?;

        Ok(())
    }

    fn allocate_page(&mut self) -> Result<u32, Error> {
        let new_page_id = self.num_pages;
        let new_count = new_page_id + 1;

        let mut new_page = Page([0; PAGE_SIZE]);
        new_page.write_u32(PAGE_ID_OFFSET, new_page_id);

        let offset = new_page_id as u64 * PAGE_SIZE as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&new_page.0)?;

        let mut page_zero = self.read_page(0)?;
        page_zero.write_u32(NUM_PAGES_OFFSET, new_count);
        self.write_page(0, page_zero)?;

        self.num_pages = new_count;

        Ok(new_page_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_U32: u32 = 0x01020304;
    const TEST_U16: u16 = 0xABCD;

    #[test]
    fn write_and_read_u32() {
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u32(0, TEST_U32);
        let read_bytes = page.read_u32(0);
        assert_eq!(TEST_U32, read_bytes);
    }

    #[test]
    fn write_and_read_u16() {
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u16(0, TEST_U16);
        let read_bytes = page.read_u16(0);
        assert_eq!(TEST_U16, read_bytes);
    }

    #[test]
    fn offset_interference() {
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u32(0, TEST_U32);
        page.write_u16(4, TEST_U16);
        assert_eq!(TEST_U32, page.read_u32(0));
        assert_eq!(TEST_U16, page.read_u16(4));
    }

    #[test]
    fn verify_byte_order() {
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u32(0, TEST_U32);
        page.write_u16(4, TEST_U16);
        assert_eq!(&page.0[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&page.0[4..6], &[0xAB, 0xCD]);
    }

    #[test]
    fn open_new_file_initializes_page_zero() {
        let path = std::env::temp_dir().join("test_page_manager_open.db");
        let _ = std::fs::remove_file(&path);

        let manager = PageManager::open(&path).unwrap();

        assert_eq!(manager.num_pages, 1);
        assert_eq!(manager.file.metadata().unwrap().len(), PAGE_SIZE as u64);

        drop(manager);

        let mut file = File::open(&path).unwrap();
        let mut page_zero = Page([0; PAGE_SIZE]);
        file.read_exact(&mut page_zero.0).unwrap();

        assert_eq!(page_zero.read_u32(PAGE_ID_OFFSET), 0);
        assert_eq!(
            page_zero.read_u16(FREE_SPACE_OFFSET),
            (PAGE_SIZE - HEADER_SIZE - 4) as u16
        );
        assert_eq!(page_zero.read_u32(NUM_PAGES_OFFSET), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reopen_rejects_file_length_mismatched_with_page_count() {
        let path = std::env::temp_dir().join("test_page_manager_reopen.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let mut page_zero = Page([0; PAGE_SIZE]);

        manager.file.seek(SeekFrom::Start(0)).unwrap();
        manager.file.read_exact(&mut page_zero.0).unwrap();

        page_zero.write_u32(NUM_PAGES_OFFSET, 7);

        manager.file.seek(SeekFrom::Start(0)).unwrap();
        manager.file.write_all(&page_zero.0).unwrap();
        manager.file.flush().unwrap();

        drop(manager);

        let err = match PageManager::open(&path) {
            Ok(_) => panic!("expected Err, got Ok"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), ErrorKind::InvalidData);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_page_zero_returns_initialized_header() {
        let path = std::env::temp_dir().join("test_read_page_zero.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let page = manager.read_page(0).unwrap();

        assert_eq!(page.read_u32(PAGE_ID_OFFSET), 0);
        assert_eq!(page.read_u32(NUM_PAGES_OFFSET), 1);
        assert_eq!(
            page.read_u16(FREE_SPACE_OFFSET),
            (PAGE_SIZE - HEADER_SIZE - 4) as u16
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_nonexistent_page_returns_error() {
        let path = std::env::temp_dir().join("test_read_missing_page.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let err = match manager.read_page(1) {
            Ok(_) => panic!("expected Err, got Ok"),
            Err(e) => e,
        };

        assert_eq!(err.kind(), ErrorKind::InvalidInput);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_page_persists_page_contents() {
        let path = std::env::temp_dir().join("test_write_page.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let mut page = manager.read_page(0).unwrap();
        page.write_u16(FREE_SPACE_OFFSET, 123);

        manager.write_page(0, page).unwrap();

        let written_page = manager.read_page(0).unwrap();
        assert_eq!(written_page.read_u16(FREE_SPACE_OFFSET), 123);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_nonexistent_page_returns_error() {
        let path = std::env::temp_dir().join("test_write_missing_page.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u32(PAGE_ID_OFFSET, 1);

        let err = manager.write_page(1, page).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_page_with_mismatched_header_returns_error() {
        let path = std::env::temp_dir().join("test_write_mismatched_page.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();
        let mut page = Page([0; PAGE_SIZE]);
        page.write_u32(PAGE_ID_OFFSET, 1);

        let err = manager.write_page(0, page).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn allocate_page_returns_next_id_and_initializes_page() {
        let path = std::env::temp_dir().join("test_allocate_page.db");
        let _ = std::fs::remove_file(&path);

        let mut manager = PageManager::open(&path).unwrap();

        let page_id = manager.allocate_page().unwrap();

        assert_eq!(page_id, 1);
        assert_eq!(manager.num_pages, 2);
        assert_eq!(
            manager.file.metadata().unwrap().len(),
            (PAGE_SIZE * 2) as u64
        );

        let page = manager.read_page(page_id).unwrap();
        assert_eq!(page.read_u32(PAGE_ID_OFFSET), page_id);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn allocated_page_count_persists_across_reopen() {
        let path = std::env::temp_dir().join("test_allocate_page_reopen.db");
        let _ = std::fs::remove_file(&path);

        {
            let mut manager = PageManager::open(&path).unwrap();
            assert_eq!(manager.allocate_page().unwrap(), 1);
            assert_eq!(manager.allocate_page().unwrap(), 2);
            assert_eq!(manager.num_pages, 3);
        }

        let reopened_manager = PageManager::open(&path).unwrap();
        assert_eq!(reopened_manager.num_pages, 3);
        assert_eq!(
            reopened_manager.file.metadata().unwrap().len(),
            (PAGE_SIZE * 3) as u64
        );

        let _ = std::fs::remove_file(&path);
    }
}
