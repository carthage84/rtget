use std::fs::{metadata, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// A file system abstraction for writing data to a file
pub struct FileSystem {
    file_path: PathBuf,
    byte_ranges: Vec<(u64, u64)>,
}

/// Implement Write for FileSystem
impl Seek for FileSystem {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file_path.seek(pos)
    }
}

/// Implement Write for FileSystem
impl FileSystem {
    // Create a new FileSystem instance
    // file_path: The path to the file to write to
    // byte_ranges: A vector of byte ranges to write to the file
    pub fn new(file_path: PathBuf, byte_ranges: Vec<(u64, u64)>) -> FileSystem {
        FileSystem {
            file_path,
            byte_ranges,
        }
    }

    // Write chunks to the file
    pub fn write_chunks(&self, chunk_data: &[(u64, Vec<u8>)]) -> io::Result<()> {
        // Iterate through the chunks and write the data to the file
        for &(start, ref data) in chunk_data {
            let mut file = OpenOptions::new().create(true).write(true).open(&self.file_path)?;
            // Seek to the start of the chunk and write the data to the file
            file.seek(SeekFrom::Start(start))?;
            file.write_all(data)?;
        }
        Ok(())
    }

    // Check if the file exists
    pub fn file_exists(&self) -> bool {
        self.file_path.exists()
    }

    // Calculate byte ranges for any existing partial files
    // Returns a vector of adjusted byte ranges
    pub async fn calculate_byte_ranges_on_existing_files(&self, byte_ranges: &mut Vec<(u64, u64)>) -> Vec<(u64, u64)> {
        // Iterate through byte ranges and adjust start and end values for any existing partial files
        for (i, (start, end)) in byte_ranges.iter_mut().enumerate() {
            let part_file_path = Path::new(self.file_path).with_file_name(format!("{}_part_{}", Path::new(self.file_path).display().to_string(), i));
            // If the partial file exists, adjust the start and end values to the end of the partial file
            if part_file_path.exists() {
                let metadata = metadata(&part_file_path).unwrap();
                let downloaded = metadata.len();
                // If the partial file is smaller than the requested range, adjust the end value to the end of the partial file
                if downloaded <= *end - *start {
                    *start += downloaded;
                } else {
                    *start = *end + 1;
                }
            }
        }
        // Return the adjusted byte ranges
        byte_ranges.clone()
    }

    // Resume a download
    // Returns an error if the file could not be opened for writing
    pub async fn resume_download(&mut self) -> io::Result<()> {
        // Adjust byte ranges for any existing partial files
        let remaining_ranges = calculate_byte_ranges_on_existing_files(&mut self.byte_ranges, &self.file_path.to_string_lossy()).await;

        // Implement logic to fetch and write the remaining data
        for (start, end) in remaining_ranges {
            // Replace this with actual data fetching logic
            let data = vec![0u8; (end - start) as usize]; // Dummy data
            let chunk_data = vec![(start, data)];
            self.write_chunks(&chunk_data)?;
        }
        Ok(())
    }
}