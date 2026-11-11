// SPDX-License-Identifier: GPL-3.0-or-later

//! Compares two images pixel by pixel, to measure a preview against a reference.

use image::RgbaImage;

/// How two images of one size differ.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub width: u32,
    pub height: u32,
    /// Pixels that are the same in both.
    pub same: u64,
    /// Pixels no channel of which differs by more than the tolerance (the same ones included).
    pub close: u64,
    /// The largest difference of any channel.
    pub largest: u8,
}

impl Comparison {
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// The share of pixels that are the same, in percent.
    pub fn same_percent(&self) -> f64 {
        100.0 * self.same as f64 / self.pixels().max(1) as f64
    }

    /// The share of pixels within the tolerance, in percent.
    pub fn close_percent(&self) -> f64 {
        100.0 * self.close as f64 / self.pixels().max(1) as f64
    }
}

/// How far two pixels are apart: the largest difference of a channel. Two fully transparent
/// pixels are the same whatever color they carry.
fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    if a[3] == 0 && b[3] == 0 {
        return 0;
    }
    (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0)
}

/// Compares `a` with `b`; `tolerance` is how far a channel may differ for a pixel to count
/// as close. With `diff`, also gives a picture of where they differ: `b` dimmed, with the
/// close pixels in yellow and the others in red.
pub fn compare(
    a: &RgbaImage,
    b: &RgbaImage,
    tolerance: u8,
    diff: bool,
) -> Result<(Comparison, Option<RgbaImage>), String> {
    if a.dimensions() != b.dimensions() {
        return Err(format!(
            "the images differ in size: {} by {} and {} by {}",
            a.width(),
            a.height(),
            b.width(),
            b.height()
        ));
    }
    let mut result = Comparison {
        width: a.width(),
        height: a.height(),
        same: 0,
        close: 0,
        largest: 0,
    };
    let mut picture = diff.then(|| RgbaImage::new(a.width(), a.height()));
    for (x, y, pa) in a.enumerate_pixels() {
        let pb = b.get_pixel(x, y);
        let d = distance(pa.0, pb.0);
        result.largest = result.largest.max(d);
        if d == 0 {
            result.same += 1;
        }
        if d <= tolerance {
            result.close += 1;
        }
        if let Some(picture) = &mut picture {
            let [r, g, b, alpha] = pb.0;
            let dim = |c: u8| (u16::from(c) * u16::from(alpha) / 255 / 3) as u8;
            let color = match d {
                0 => [dim(r), dim(g), dim(b), 255],
                d if d <= tolerance => [255, 220, 0, 255],
                _ => [255, 0, 0, 255],
            };
            picture.put_pixel(x, y, image::Rgba(color));
        }
    }
    Ok((result, picture))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn it_counts_same_and_close_pixels() {
        let a = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));
        let mut b = a.clone();
        b.put_pixel(0, 0, Rgba([12, 20, 30, 255]));
        b.put_pixel(1, 1, Rgba([200, 20, 30, 255]));
        let (c, picture) = compare(&a, &b, 4, true).unwrap();
        assert_eq!((c.same, c.close, c.largest), (2, 3, 190));
        assert_eq!(c.same_percent(), 50.0);
        let picture = picture.unwrap();
        assert_eq!(picture.get_pixel(0, 0).0, [255, 220, 0, 255]);
        assert_eq!(picture.get_pixel(1, 1).0, [255, 0, 0, 255]);
    }

    #[test]
    fn transparent_pixels_are_the_same_whatever_their_color() {
        let a = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0]));
        let b = RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 0]));
        assert_eq!(compare(&a, &b, 0, false).unwrap().0.same, 1);
    }

    #[test]
    fn images_of_different_sizes_do_not_compare() {
        let a = RgbaImage::new(1, 1);
        let b = RgbaImage::new(2, 1);
        assert!(compare(&a, &b, 0, false).is_err());
    }
}
