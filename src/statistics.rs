use std::collections::HashMap;

/// A simple struct to keep track of amount of data sent by MIME type.
#[derive(Debug, Default)]
pub struct Statistics {
    statmap: HashMap<String, usize>,
}

impl Statistics {
    pub fn add<S: Into<String>>(&mut self, mime: S, size: usize) {
        let mime = mime.into();

        match self.statmap.get_mut(&mime) {
            Some(total) => *total += size,
            None => {
                self.statmap.insert(mime, size);
            }
        }
    }

    /// Get how many bytes were sent for a specific mime type. If the mime type
    /// is not in the map, meaning it's not been sent out before, 0 is returned.
    pub fn sent<S: Into<String>>(&self, mime: S) -> usize {
        self.statmap
            .get(&mime.into())
            .map(|size| *size)
            .unwrap_or_default()
    }

    /// Get the total number of bytes that have been sent with a mime type
    /// starting in "image/"
    pub fn image(&self) -> usize {
        self.statmap.iter().fold(0, |acc, (mime, &total)| {
            if mime.starts_with("image") {
                acc + total
            } else {
                acc
            }
        })
    }

    /// Shorthand for querying the "text/html" mime type
    pub fn html(&self) -> usize {
        self.statmap
            .get("text/html")
            .map(|size| *size)
            .unwrap_or_default()
    }
}
