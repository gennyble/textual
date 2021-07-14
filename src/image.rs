use crate::color::Color;

#[derive(Debug, PartialEq)]
pub enum Colors {
    RGBA,
    RGB,
    Grey,
    GreyAsAlpha(Color),
}

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

    pub fn from_buffer(width: usize, height: usize, mut data: Vec<u8>, colors: Colors) -> Self {
        let expected_len = match colors {
            Colors::Grey | Colors::GreyAsAlpha(_) => width * height,
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

        if let Colors::GreyAsAlpha(color) = colors {
            let mut colordata = Vec::with_capacity(width * height * 4);
            for byte in data.into_iter() {
                colordata.extend_from_slice(&[color.r, color.g, color.b, byte]);
            }
            data = colordata;
        } else if colors == Colors::Grey {
            let mut colordata = Vec::with_capacity(width * height * 4);
            for byte in data.into_iter() {
                colordata.extend_from_slice(&[byte, byte, byte, byte]);
            }
            data = colordata;
        } else if colors == Colors::RGBA {
            let mut colordata = Vec::with_capacity(width * height * 4);
            for rgb in data.chunks(3) {
                colordata.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255])
            }
            data = colordata;
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
