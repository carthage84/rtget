use reqwest;
use reqwest::Client;
use crate::error::AppError;

pub async fn download(client: &Client, url: &str, start: usize, end: usize) -> Result<(), String> {
    // Perform FTP request
    match client.get(url).header("Range", format!("bytes={}-{}", start, end)).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();
                Ok(())
            } else {
                Err(AppError::CouldNotConnect(response.status().to_string())).unwrap()}
        }
        Err(e) => Err(AppError::CouldNotConnect(e.to_string())).unwrap(),
    }
}

pub async fn get_total_file_size(client: &Client, url: &str) -> Result<usize, String> {
    match client.head(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Some(content_length) = response.headers().get(reqwest::header::CONTENT_LENGTH) {
                    if let Ok(content_length_str) = content_length.to_str() {
                        if let Ok(size) = content_length_str.parse::<usize>() {
                            return Ok(size);
                        }
                    }
                }
                Err("Failed to parse content length".to_string())
            } else {
                Err(AppError::CouldNotConnect(response.status().to_string())).unwrap()
            }
        }
        Err(e) => Err(AppError::CouldNotConnect(e.to_string())).unwrap(),
    }
}