use crate::search::{SearchResultItem, ValueType};
use crate::search::result_manager::SearchResultManager;
use log::{debug, info};
use memmap2::MmapMut;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

#[repr(packed)]
#[derive(Debug, Clone, Copy)]
pub struct ExactSearchResultItem {
    pub address: u64,
    pub typ: ValueType,
}

impl From<(u64, ValueType)> for ExactSearchResultItem {
    fn from(tuple: (u64, ValueType)) -> Self {
        ExactSearchResultItem {
            address: tuple.0,
            typ: tuple.1,
        }
    }
}

pub struct ExactSearchResultManager {
    memory_buffer: Vec<ExactSearchResultItem>,
    memory_buffer_capacity: usize,
    cache_dir: PathBuf,
    disk_file_path: Option<PathBuf>,
    disk_file: Option<File>,
    mmap: Option<MmapMut>,
    disk_count: usize,
    total_count: usize,
}

impl ExactSearchResultManager {
    pub fn new(memory_buffer_size: usize, cache_dir: PathBuf) -> Self {
        let capacity = if memory_buffer_size == 0 {
            0
        } else {
            memory_buffer_size / size_of::<ExactSearchResultItem>()
        };

        if memory_buffer_size == 0 {
            info!(
                "Initializing CompactSearchResultManager: memory_buffer_capacity=0 (direct disk write mode), cache_dir={:?}",
                cache_dir
            );
        } else {
            info!(
                "Initializing CompactSearchResultManager: memory_buffer_capacity={} items ({} MB), cache_dir={:?}",
                capacity,
                memory_buffer_size / 1024 / 1024,
                cache_dir
            );
        }

        ExactSearchResultManager {
            memory_buffer: Vec::with_capacity(capacity),
            memory_buffer_capacity: capacity,
            cache_dir,
            disk_file_path: None,
            disk_file: None,
            mmap: None,
            disk_count: 0,
            total_count: 0,
        }
    }

    pub fn clear(&mut self) -> anyhow::Result<()> {
        self.memory_buffer.clear();
        self.total_count = 0;
        self.disk_count = 0;

        debug!("Search results cleared (disk file and resources preserved for reuse)");
        Ok(())
    }

    pub fn destroy(&mut self) -> anyhow::Result<()> {
        self.memory_buffer.clear();
        self.total_count = 0;
        self.disk_count = 0;

        if let Some(ref path) = self.disk_file_path {
            drop(self.mmap.take());
            drop(self.disk_file.take());
            if path.exists() {
                std::fs::remove_file(path)?;
                debug!("Removed disk file: {:?}", path);
            }
        }

        self.disk_file_path = None;
        info!("CompactSearchResultManager destroyed");
        Ok(())
    }

    pub fn add_result(&mut self, item: ExactSearchResultItem) -> anyhow::Result<()> {
        if self.memory_buffer_capacity == 0 {
            self.write_to_disk(&item)?;
        } else if self.memory_buffer.len() < self.memory_buffer_capacity {
            self.memory_buffer.push(item);
        } else {
            self.write_to_disk(&item)?;
        }

        self.total_count += 1;
        Ok(())
    }

    fn write_to_disk(&mut self, item: &ExactSearchResultItem) -> anyhow::Result<()> {
        if self.disk_file.is_none() {
            self.init_disk_file()?;
        }

        if let Some(ref mut mmap) = self.mmap {
            let offset = self.disk_count * size_of::<ExactSearchResultItem>();
            let mmap_size = mmap.len();

            if offset + size_of::<ExactSearchResultItem>() > mmap_size {
                drop(self.mmap.take());
                let new_size = mmap_size + 128 * 1024 * 1024;
                if let Some(ref file) = self.disk_file {
                    file.set_len(new_size as u64)?;
                }
                self.mmap = Some(unsafe { MmapMut::map_mut(self.disk_file.as_ref().unwrap())? });
            }

            let mmap = self.mmap.as_mut().unwrap();
            unsafe {
                let ptr = mmap.as_mut_ptr().add(offset) as *mut ExactSearchResultItem;
                ptr.write(*item);
            }

            self.disk_count += 1;
        }

        Ok(())
    }

    fn init_disk_file(&mut self) -> anyhow::Result<()> {
        let file_path = self.cache_dir.join("mamu_search_results.bin");

        debug!("Creating disk file: {:?}", file_path);

        let initial_size = 128 * 1024 * 1024;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&file_path)?;

        file.set_len(initial_size as u64)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        self.disk_file_path = Some(file_path);
        self.disk_file = Some(file);
        self.mmap = Some(mmap);

        info!("Disk file initialized with size {} MB", initial_size / 1024 / 1024);
        Ok(())
    }
    
    pub fn clear_disk(&mut self) -> anyhow::Result<()> {
        drop(self.mmap.take());
        drop(self.disk_file.take());

        if let Some(ref path) = self.disk_file_path {
            if path.exists() {
                std::fs::remove_file(path)?;
                debug!("Removed disk file: {:?}", path);
            }
        }

        self.disk_file_path = None;
        self.disk_count = 0;

        info!("Disk resources cleared");
        Ok(())
    }

    pub fn get_results(&self, start: usize, size: usize) -> anyhow::Result<Vec<ExactSearchResultItem>> {
        let end = std::cmp::min(start + size, self.total_count);
        if start >= self.total_count {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(end - start);

        for i in start..end {
            if i < self.memory_buffer.len() {
                results.push(self.memory_buffer[i]);
            } else {
                let disk_index = i - self.memory_buffer.len();
                if let Some(ref mmap) = self.mmap {
                    let offset = disk_index * size_of::<ExactSearchResultItem>();
                    unsafe {
                        let ptr = mmap.as_ptr().add(offset) as *const ExactSearchResultItem;
                        results.push(*ptr);
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn total_count(&self) -> usize {
        self.total_count
    }

    pub fn memory_count(&self) -> usize {
        self.memory_buffer.len()
    }

    pub fn disk_count(&self) -> usize {
        self.disk_count
    }

    pub fn remove_result(&mut self, index: usize) -> anyhow::Result<()> {
        if index >= self.total_count {
            return Err(anyhow::anyhow!("Index out of bounds: {} >= {}", index, self.total_count));
        }

        if index < self.memory_buffer.len() {
            // 删除内存中的数据
            self.memory_buffer.remove(index);
        } else {
            // 删除磁盘中的数据，需要移动后面的数据
            let disk_index = index - self.memory_buffer.len();
            self.remove_disk_item(disk_index)?;
        }

        self.total_count -= 1;
        debug!("Removed result at index {}, total count: {}", index, self.total_count);
        Ok(())
    }

    fn remove_disk_item(&mut self, disk_index: usize) -> anyhow::Result<()> {
        if disk_index >= self.disk_count {
            return Err(anyhow::anyhow!("Disk index out of bounds"));
        }

        if let Some(ref mut mmap) = self.mmap {
            let item_size = size_of::<ExactSearchResultItem>();
            let src_offset = (disk_index + 1) * item_size;
            let dst_offset = disk_index * item_size;
            let move_count = self.disk_count - disk_index - 1;

            if move_count > 0 {
                unsafe {
                    let src = mmap.as_ptr().add(src_offset);
                    let dst = mmap.as_mut_ptr().add(dst_offset);
                    std::ptr::copy(src, dst, move_count * item_size);
                }
            }

            self.disk_count -= 1;
        }

        Ok(())
    }

    pub fn remove_results_batch(&mut self, mut indices: Vec<usize>) -> anyhow::Result<()> {
        // 从大到小排序，这样删除时不会影响后续的索引
        indices.sort_unstable_by(|a, b| b.cmp(a));

        for &index in &indices {
            self.remove_result(index)?;
        }

        debug!("Removed {} results in batch, total count: {}", indices.len(), self.total_count);
        Ok(())
    }

    /// Get all results (used for refine search)
    pub fn get_all_results(&self) -> anyhow::Result<Vec<ExactSearchResultItem>> {
        self.get_results(0, self.total_count)
    }
}

impl Drop for ExactSearchResultManager {
    fn drop(&mut self) {
        let _ = self.destroy();
    }
}
