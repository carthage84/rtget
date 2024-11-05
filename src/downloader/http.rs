use reqwest;
use reqwest::Client;
use crate::error::AppError;

// Download a file from an HTTP URL
// Returns an error message if the download failed
pub async fn download(client: &Client, url: &str, start: usize, end: usize) -> Result<(), String> {
    // Perform HTTP request
    match client.get(url).header("Range", format!("bytes={}-{}", start, end)).send().await {
        // If the request was successful, return the response body as a stream
        Ok(response) => {
            // If the request was successful, return the response body as a stream
            if response.status().is_success() {
                let mut stream = response.bytes_stream();
                Ok(())
            } else {
                // If the request was not successful, return an error message
                Err(AppError::CouldNotConnect(response.status().to_string())).unwrap()
            }
        }
        // If the request was not successful, return an error message
        Err(e) => Err(AppError::CouldNotConnect(e.to_string())).unwrap(),
    }

}

// Get the total file size from the HTTP response headers
// Returns the total file size in bytes as an usize or an error message if the size could not be parsed
pub async fn get_total_file_size(client: &Client, url: &str) -> Result<usize, String> {
    // Perform HTTP request
    match client.head(url).send().await {
        // If the request was successful,
        // parse the content length header and return the size in bytes
        Ok(response) => {
            // If the request was successful,
            // parse the content length header and return the size in bytes
            if response.status().is_success() {
                // Get the content length header value as a string
                response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .ok_or("Could not parse content length". to_string())
            } else {
                // If the request was not successful, return an error message
                Err(AppError::CouldNotConnect(response.status().to_string())).unwrap()
            }
        }
        // If the request was not successful, return an error message
        Err(e) => Err(AppError::CouldNotConnect(e.to_string())).unwrap(),
    }
}