use std::sync::Arc;

use crate::color::Color;

pub trait ColorProvider: Send + Sync {
    fn color_at(&self, x: usize, y: usize) -> Color;
}

pub struct Stripes {
    pub colors: Vec<Color>,
    pub stripe_width: usize,
    pub slope: f32,
}

impl ColorProvider for Stripes {
    fn color_at(&self, x: usize, y: usize) -> Color {
        let color_index =
            ((x + (y as f32 / self.slope) as usize) / self.stripe_width) % self.colors.len();

        self.colors[color_index]
    }
}

pub enum Colors<'a> {
    RGBA,
    RGB,
    Grey,
    GreyAsAlpha(Color),
    GreyAsMask(&'a dyn ColorProvider),
}

#[derive(Debug, Clone)]
pub struct Image {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self::with_color(width, height, Color::BLACK)
    }

    pub fn with_color(width: usize, height: usize, color: Color) -> Self {
        let data = Into::<[u8; 4]>::into(color).repeat(width * height);

        Self {
            width,
            height,
            data,
        }
    }

    pub fn from_provider<CP: ColorProvider + ?Sized>(
        width: usize,
        height: usize,
        off_x: isize,
        off_y: isize,
        cp: &CP,
    ) -> Image {
        let mut data = vec![0; width * height * 4];

        for index in 0..width * height {
            let color = cp.color_at(
                ((index % width) as isize + off_x) as usize,
                ((index / width) as isize + off_y) as usize,
            );

            data[index * 4] = color.r;
            data[index * 4 + 1] = color.g;
            data[index * 4 + 2] = color.b;
            data[index * 4 + 3] = color.a;
        }

        Self {
            width,
            height,
            data,
        }
    }

    pub fn from_buffer(width: usize, height: usize, mut data: Vec<u8>, colors: Colors) -> Self {
        let expected_len = match colors {
            Colors::Grey | Colors::GreyAsAlpha(_) | Colors::GreyAsMask(_) => width * height,
            Colors::RGB => width * height * 3,
            Colors::RGBA => width * height * 4,
        };

        if data.len() != expected_len {
            panic!(
                "Expected length to be {} but it's {}",
                expected_len,
                data.len()
            );
        }

        match colors {
            Colors::Grey => {
                let mut colordata = Vec::with_capacity(width * height * 4);
                for byte in data.into_iter() {
                    colordata.extend_from_slice(&[byte, byte, byte, byte]);
                }
                data = colordata;
            }
            Colors::GreyAsAlpha(color) => {
                let mut colordata = Vec::with_capacity(width * height * 4);
                for byte in data.into_iter() {
                    colordata.extend_from_slice(&[color.r, color.g, color.b, byte]);
                }
                data = colordata;
            }
            Colors::GreyAsMask(provider) => {
                let mut colordata = Vec::with_capacity(width * height * 4);
                for (index, byte) in data.into_iter().enumerate() {
                    let color = provider.color_at(index % width, index / width);
                    colordata.extend_from_slice(&[color.r, color.g, color.b, byte]);
                }
                data = colordata;
            }
            Colors::RGB => {
                let mut colordata = Vec::with_capacity(width * height * 4);
                for rgb in data.chunks(3) {
                    colordata.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255])
                }
                data = colordata;
            }
            Colors::RGBA => (),
        }

        Self {
            width,
            height,
            data,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn xy_to_index(&self, x: usize, y: usize) -> usize {
        (y as usize * self.width + x) * 4
    }

    pub fn color(&self, x: usize, y: usize) -> Color {
        //todo: don't assume xy inbounds
        let i = self.xy_to_index(x, y);

        Color {
            r: self.data[i],
            g: self.data[i + 1],
            b: self.data[i + 2],
            a: self.data[i + 3],
        }
    }

    pub fn set_color(&mut self, x: usize, y: usize, color: Color) {
        //todo: don't assume xy inbounds
        let i = self.xy_to_index(x, y);

        self.data[i] = color.r;
        self.data[i + 1] = color.g;
        self.data[i + 2] = color.b;
        self.data[i + 3] = color.a;
    }

    pub fn draw_img(&mut self, img: Image, off_x: isize, off_y: isize) {
        let img_data = img.data();
        for img_y in 0..(img.height() as isize) {
            // current pixel y value
            let y = off_y + img_y;

            if y < 0 {
                // Less than 0? Could still come into bounds
                continue;
            } else if y >= self.height as isize {
                // If the pixel Y is greater than the height, it's over
                return;
            }

            for img_x in 0..(img.width() as isize) {
                // Current pixel x value
                let x = off_x + img_x;

                if x < 0 {
                    continue;
                }
                if x >= self.width as isize {
                    break;
                } else {
                    let img_index = img.xy_to_index(img_x as usize, img_y as usize);
                    let our_index = self.xy_to_index(x as usize, y as usize);

                    let nrml = |c: u8| c as f32 / 255.0;

                    let img_alpha_float = img_data[img_index + 3] as f32 / 255.0;
                    let our_alpha_float = self.data[our_index + 3] as f32 / 255.0;
                    let mixed_alpha = img_alpha_float + our_alpha_float * (1.0 - img_alpha_float);

                    let mix = |color_under: u8, color_over: u8| {
                        let nrml_over = nrml(color_over);
                        let nrml_under = nrml(color_under);

                        if img_alpha_float == 0.0 {
                            color_under
                        } else if our_alpha_float == 0.0 {
                            color_over
                        } else {
                            ((((nrml_over * img_alpha_float)
                                + ((nrml_under * our_alpha_float) * (1.0 - img_alpha_float)))
                                / mixed_alpha)
                                * 255.0) as u8
                        }
                    };

                    self.data[our_index] = mix(self.data[our_index], img_data[img_index]);
                    self.data[our_index + 1] =
                        mix(self.data[our_index + 1], img_data[img_index + 1]);
                    self.data[our_index + 2] =
                        mix(self.data[our_index + 2], img_data[img_index + 2]);
                    self.data[our_index + 3] = (mixed_alpha * 255.0) as u8;
                }
            }
        }
    }

    pub fn mask(&mut self, mask: Mask, off_x: isize, off_y: isize) {
        for img_y in 0..(mask.height as isize) {
            let y = off_y + img_y;

            if y < 0 {
                continue;
            } else if y >= self.height as isize {
                return;
            }

            for img_x in 0..(mask.width as isize) {
                let x = off_x + img_x;

                if x < 0 {
                    continue;
                }
                if x >= self.width as isize {
                    break;
                } else {
                    let i = self.xy_to_index(x as usize, y as usize);
                    self.data[i + 3] = mask.data[(img_y * mask.width as isize + img_x) as usize];
                }
            }
        }
    }

    pub fn overlay(&mut self, overlaid: Image, mask: Mask, off_x: isize, off_y: isize) {
        if overlaid.width != mask.width || overlaid.height != mask.height {
            // Overlay and mask need to be the same size
            return; //todo: error here
        }

        for img_y in 0..(overlaid.height() as isize) {
            let y = off_y + img_y;

            if y < 0 {
                continue;
            } else if y >= self.height as isize {
                return;
            }

            for img_x in 0..(overlaid.width() as isize) {
                let x = off_x + img_x;

                if x < 0 {
                    continue;
                }
                if x >= self.width as isize {
                    break;
                } else {
                    let mut over_color = overlaid.color(img_x as usize, img_y as usize);
                    let our_color = self.color(x as usize, y as usize);
                    over_color.a = 255 - mask.data[(img_y * mask.width as isize + img_x) as usize];

                    self.set_color(x as usize, y as usize, Self::mix(our_color, over_color));
                }
            }
        }
    }

    pub fn mix(color_under: Color, color_over: Color) -> Color {
        let nrml = |c: u8| c as f32 / 255.0;
        let img_alpha_float = color_over.a as f32 / 255.0;
        let our_alpha_float = color_under.a as f32 / 255.0;
        let mixed_alpha = img_alpha_float + our_alpha_float * (1.0 - img_alpha_float);

        let mix_component = |color_under: u8, color_over: u8| {
            let nrml_over = nrml(color_over);
            let nrml_under = nrml(color_under);

            if img_alpha_float == 0.0 {
                color_under
            } else if our_alpha_float == 0.0 {
                color_over
            } else {
                ((((nrml_over * img_alpha_float)
                    + ((nrml_under * our_alpha_float) * (1.0 - img_alpha_float)))
                    / mixed_alpha)
                    * 255.0) as u8
            }
        };

        Color {
            r: mix_component(color_under.r, color_over.r),
            g: mix_component(color_under.g, color_over.g),
            b: mix_component(color_under.b, color_over.b),
            a: (mixed_alpha * 255.0) as u8,
        }
    }

    pub fn horizontal_line(&mut self, x: usize, y: usize, len: usize, color: Color) {
        for i in 0..len {
            // TODO: Check x and y are valid coordiantes
            let index = self.xy_to_index(x + i, y);

            self.data[index] = color.r;
            self.data[index + 1] = color.g;
            self.data[index + 2] = color.b;
            self.data[index + 3] = color.a;
        }
    }

    pub fn vertical_line(&mut self, x: usize, y: usize, len: usize, color: Color) {
        for i in 0..len {
            // TODO: Check x and y are valid coordiantes
            let index = self.xy_to_index(x, y + i);

            self.data[index] = color.r;
            self.data[index + 1] = color.g;
            self.data[index + 2] = color.b;
            self.data[index + 3] = color.a;
        }
    }

    pub fn rect(&mut self, x1: usize, y1: usize, width: usize, height: usize, color: Color) {
        self.vertical_line(x1, y1, height, color); //Right
        self.horizontal_line(x1, y1, width, color); //Top
        self.vertical_line(x1 + width, y1, height, color); //Left
        self.horizontal_line(x1, y1 + height, width, color); //Bottom
    }
}

/// A greyscale image.
pub struct Mask {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Mask {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0; width * height],
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn set_from_buf(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        off_x: isize,
        off_y: isize,
    ) {
        for buf_y in 0..(height as isize) {
            let y = off_y + buf_y as isize;

            if y < 0 {
                continue; // Might come in bounds
            } else if y >= self.height as isize {
                return; // It's over
            }

            for buf_x in 0..(width as isize) {
                let x = off_x + buf_x as isize;

                if x < 0 {
                    continue; // Might come in bounds
                } else if x >= self.width as isize {
                    break; // It's over
                } else {
                    self.data[(y * self.width as isize + x) as usize] =
                        buf[(buf_y * width as isize + buf_x) as usize];
                }
            }
        }
    }
}
