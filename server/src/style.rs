//TODO: gen- all of this
pub trait ImageStyle {
	pub fn new() -> Result<Self, ImageStyleError>;
	pub fn generate();
}
