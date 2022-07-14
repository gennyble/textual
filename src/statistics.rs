use std::collections::HashMap;

/// A simple struct to keep track of amount of data sent by MIME type.
#[derive(Debug, Default)]
pub struct Statistics {
	statmap: HashMap<String, usize>,
	requests: usize,
}

impl Statistics {
	/// Takes a mime type and a number of bytes. Each call of this function is
	/// considered a separate request for the purposes of [Statistics::requsets]
	pub fn add<S: Into<String>>(&mut self, mime: S, size: usize) {
		let mime = mime.into();

		match self.statmap.get_mut(&mime) {
			Some(total) => *total += size,
			None => {
				self.statmap.insert(mime, size);
			}
		}

		self.requests += 1;
	}

	/// Get the number of requests the server has seen total since boot up. Each call
	/// to [Statistics::add] increments the requests count by one.
	pub fn requests(&self) -> usize {
		self.requests
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
