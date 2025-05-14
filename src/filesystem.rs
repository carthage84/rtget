use tokio::io::AsyncReadExt;
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};
use crate::error::AppError;
use log::{debug, info};

/// A file system abstraction for writing data to a file
pub struct FileSystem {
    file_path: PathBuf,
    byte_ranges: Vec<(u64, u64)>,
    file: Option<File>,
}

/// Implement Write for FileSystem
impl FileSystem {
    // Create a new FileSystem instance
    // file_path: The path to the file to write to
    // byte_ranges: A vector of byte ranges to write to the file
    pub fn new(file_path: &Path, byte_ranges: Vec<(usize, usize)>) -> Self {
        FileSystem {
            file_path: file_path.to_path_buf(),
            byte_ranges: byte_ranges
                .into_iter()
                .map(|(start, end)| (start as u64, end as u64))
                .collect(),
            file: None,
        }
    }

    /// Asynchronously creates a file at the specified file path and assigns it to the `file` field in the struct.
    ///
    /// # Returns
    ///
    /// If successful, returns a mutable reference to `self` wrapped in a `Result`.
    /// If an error occurs during file creation, returns an `AppError::CouldNotConnect` with the error message.
    ///
    /// # Errors
    ///
    /// This function will return an error of type `AppError::CouldNotConnect` if the file cannot be created,
    /// with the underlying error's string representation.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut instance = MyStruct { file_path: "example.txt".to_string(), file: None };
    /// if let Err(error) = instance.create_file().await {
    ///     eprintln!("Failed to create file: {:?}", error);
    /// } else {
    ///     println!("File created successfully!");
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// The `self.file_path` must contain a valid file path, and the function requires the `async` runtime
    /// to support asynchronous file creation using `tokio::fs::File::create`.
    pub async fn create_file(&mut self) -> Result<&mut Self, AppError> {
        let file = File::create(&self.file_path)
            .await
            .map_err(|e| AppError::CouldNotConnect(e.to_string()))?;
        self.file = Some(file);
        Ok(self)
    }

    /// Writes a chunk of data to a specified start position in the file.
    ///
    /// This asynchronous function writes the given byte slice (`chunk`) to the file at the provided
    /// starting position (`start`). The file must already be initialized; otherwise, an error will
    /// be returned. Any I/O errors encountered during seeking or writing will also result in an error.
    ///
    /// # Parameters
    /// - `chunk`: A reference to a byte slice (`&[u8]`) containing the data to be written.
    /// - `start`: A `u64` value indicating the offset from the start of the file where the write operation should begin.
    ///
    /// # Returns
    /// - `Ok(())`: If the chunk was successfully written to the file.
    /// - `Err(AppError)`: If the file is not initialized or an error occurs during the seek or write operation.
    ///
    /// # Errors
    /// Returns an `AppError::CouldNotConnect` in the following cases:
    /// - If the file has not been initialized.
    /// - If there is an error seeking to the specified position in the file.
    /// - If there is an error writing the chunk to the file.
    ///
    /// # Example
    /// ```
    /// use my_crate::AppError;
    ///
    /// async fn example_usage() -> Result<(), AppError> {
    ///     let mut file_writer = FileWriter::new().await?;
    ///     let data = b"Hello, world!";
    ///     let start_position = 0;
    ///
    ///     file_writer.write_chunk(data, start_position).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Note
    /// This function assumes that the `file` field in the struct is an `Option` wrapping a type
    /// that implements both the `AsyncSeek` and `AsyncWrite` traits.
    pub async fn write_chunk(&mut self, chunk: &[u8], start: u64, part_start: u64, max_size: u64) -> Result<usize, AppError> {
        if let Some(file) = &mut self.file {
            let part_offset = start - part_start;
            if part_offset >= max_size {
                debug!("Skipping chunk for part at offset {} (part offset {}): exceeds max size {}", start, part_offset, max_size);
                return Ok(0);
            }
            let write_size = (max_size - part_offset).min(chunk.len() as u64) as usize;
            debug!("Writing chunk at offset {} (part offset {}): {} bytes (of {})", start, part_offset, write_size, chunk.len());
            file.seek(SeekFrom::Start(part_offset)).await
                .map_err(|e| AppError::CouldNotConnect(format!("Failed to seek to {}: {}", part_offset, e)))?;
            file.write_all(&chunk[..write_size]).await
                .map_err(|e| AppError::CouldNotConnect(format!("Failed to write chunk: {}", e)))?;
            Ok(write_size)
        } else {
            debug!("Error: File not initialized for part");
            Err(AppError::CouldNotConnect("File not initialized".to_string()))
        }
    }

    /// Checks whether the file path associated with the instance exists on the filesystem.
    ///
    /// # Returns
    ///
    /// * `true` if the file at the specified path exists.
    /// * `false` if the file does not exist.
    ///
    /// # Example
    ///
    /// ```rust
    /// let file_checker = FileChecker {
    ///     file_path: PathBuf::from("example.txt"),
    /// };
    ///
    /// if file_checker.file_exists() {
    ///     println!("The file exists!");
    /// } else {
    ///     println!("The file does not exist!");
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// This method relies on `std::path::Path::exists` to determine the existence of the file
    /// and does not differentiate between files and directories.
    pub fn file_exists(&self) -> bool {
        self.file_path.exists()
    }

    // Calculate byte ranges for any existing partial files
    // Returns a vector of adjusted byte ranges
    pub async fn calculate_byte_ranges_on_existing_files(
        &self,
        byte_ranges: &mut Vec<(u64, u64)>,
    ) -> Vec<(u64, u64)> {
        for (i, (start, end)) in byte_ranges.iter_mut().enumerate() {
            let part_file_path = Path::new(&self.file_path)
                .with_file_name(format!("{}_part_{}", self.file_path.display(), i));
            if part_file_path.exists() {
                let metadata = std::fs::metadata(&part_file_path)
                    .map_err(|e| AppError::CouldNotConnect(e.to_string()))
                    .unwrap();
                let downloaded = metadata.len();
                if downloaded <= *end - *start {
                    *start += downloaded;
                } else {
                    *start = *end + 1;
                }
            }
        }
        byte_ranges.clone()
    }

    pub async fn merge_chunks(&self, output_path: &Path, num_chunks: u8) -> Result<(), AppError> {
        // Create or open the output file
        let mut output_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true) // Overwrite existing file
            .open(output_path)
            .await
            .map_err(|e| AppError::CouldNotConnect(format!("Failed to create output file: {}", e)))?;

        // Iterate over partial files
        for i in 0..num_chunks {
            let part_file_path = self
                .file_path
                .with_file_name(format!("{}_part_{}", self.file_path.display(), i));

            // Check if partial file exists
            if !part_file_path.exists() {
                return Err(AppError::CouldNotConnect(format!(
                    "Partial file missing: {}",
                    part_file_path.display()
                )));
            }

            // Open and read partial file
            let mut part_file = File::open(&part_file_path)
                .await
                .map_err(|e| AppError::CouldNotConnect(format!("Failed to open partial file {}: {}", part_file_path.display(), e)))?;

            // Read contents into buffer
            let mut buffer = Vec::new();
            part_file
                .read_to_end(&mut buffer)
                .await
                .map_err(|e| AppError::CouldNotConnect(format!("Failed to read partial file {}: {}", part_file_path.display(), e)))?;

            // Write to output file
            output_file
                .write_all(&buffer)
                .await
                .map_err(|e| AppError::CouldNotConnect(format!("Failed to write to output file: {}", e)))?;

            //println!("Merged partial file: {}", part_file_path.display());
        }

        // Ensure all data is written to disk
        output_file
            .flush()
            .await
            .map_err(|e| AppError::CouldNotConnect(format!("Failed to flush output file: {}", e)))?;

        // Cleanup: Delete partial files
        for i in 0..num_chunks {
            let part_file_path = self
                .file_path
                .with_file_name(format!("{}_part_{}", self.file_path.display(), i));
            if part_file_path.exists() {
                tokio::fs::remove_file(&part_file_path)
                    .await
                    .map_err(|e| AppError::CouldNotConnect(format!("Failed to delete partial file {}: {}", part_file_path.display(), e)))?;
                //println!("Deleted partial file: {}", part_file_path.display());
            }
        }

        Ok(())
    }
}
