//! Bounded response collection for clients that read whole HTTP bodies.

use reqwest::Response;

#[derive(Debug)]
pub enum BodyReadError {
    Transfer,
    TooLarge,
}

pub async fn read_bounded(mut response: Response, limit: usize) -> Result<Vec<u8>, BodyReadError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| BodyReadError::Transfer)?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(BodyReadError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
