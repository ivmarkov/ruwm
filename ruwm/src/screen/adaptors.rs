use core::cmp::{max, min};
use core::convert::Infallible;
use core::fmt::Debug;
use core::marker::PhantomData;

use log::trace;

use embedded_graphics::draw_target::{
    Clipped, ColorConverted, Cropped, DrawTarget, DrawTargetExt, Translated,
};
use embedded_graphics::prelude::{Dimensions, IntoStorage, PixelColor, Point, RawData, Size};
use embedded_graphics::primitives::{PointsIter, Rectangle};
use embedded_graphics::Pixel;

//
// Owned
//

pub trait Transformer {
    type Color: PixelColor;
    type Error;

    type DrawTarget<'a>: DrawTarget<Color = Self::Color, Error = Self::Error>
    where
        Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a>;

    fn into_owned(self) -> Owned<Self>
    where
        Self: Sized,
    {
        Owned::new(self)
    }
}

pub struct TranslatedT<T>(T, Point);

impl<T> Transformer for TranslatedT<T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Translated<'a, T> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.translated(self.1)
    }
}

pub struct CroppedT<T>(T, Rectangle);

impl<T> Transformer for CroppedT<T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Cropped<'a, T> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.cropped(&self.1)
    }
}

pub struct ClippedT<T>(T, Rectangle);

impl<T> Transformer for ClippedT<T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Clipped<'a, T> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.clipped(&self.1)
    }
}

pub struct ColorConvertedT<T, C>(T, PhantomData<C>);

impl<T, C> Transformer for ColorConvertedT<T, C>
where
    T: DrawTarget,
    C: PixelColor + Into<T::Color>,
{
    type Color = C;
    type Error = T::Error;

    type DrawTarget<'a> = ColorConverted<'a, T, C> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.color_converted()
    }
}

pub struct RotatedT<T>(T, RotateAngle);

impl<T> Transformer for RotatedT<T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Rotated<'a, T> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.rotated(self.1)
    }
}

pub struct ScaledT<T>(T, Size);

impl<T> Transformer for ScaledT<T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Scaled<'a, T> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.scaled(self.1)
    }
}

pub struct FlushingT<T, F>(T, F);

impl<T, F> Transformer for FlushingT<T, F>
where
    T: DrawTarget + 'static,
    F: FnMut(&mut T) -> Result<(), T::Error> + Send + Clone + 'static,
{
    type Color = T::Color;
    type Error = T::Error;

    type DrawTarget<'a> = Flushing<'a, T, F> where Self: 'a;

    fn transform<'a>(&'a mut self) -> Self::DrawTarget<'a> {
        self.0.flushing(self.1.clone())
    }
}

pub struct Owned<T>(T, Rectangle);

impl<T> Owned<T>
where
    T: Transformer,
{
    fn new(mut transformer: T) -> Self {
        let bbox = transformer.transform().bounding_box();

        Self(transformer, bbox)
    }
}

impl<T> DrawTarget for Owned<T>
where
    T: Transformer,
{
    type Color = T::Color;
    type Error = T::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.0.transform().draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.0.transform().fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.0.transform().fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.0.transform().clear(color)
    }
}

impl<T> Dimensions for Owned<T>
where
    T: Transformer,
{
    fn bounding_box(&self) -> Rectangle {
        self.1
    }
}

impl<T> Flushable for Owned<T>
where
    T: Transformer,
    for<'a> T::DrawTarget<'a>: Flushable,
{
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.transform().flush()
    }
}

//
// Flushable
//

pub trait Flushable: DrawTarget {
    fn flush(&mut self) -> Result<(), Self::Error>;
}

pub struct Flushing<'a, T, F> {
    parent: &'a mut T,
    flusher: F,
}

impl<'a, T, F> Flushing<'a, T, F> {
    pub fn new(parent: &'a mut T, flusher: F) -> Self {
        Self { parent, flusher }
    }
}

impl<'a, T> Flushing<'a, T, fn(&mut T) -> Result<(), T::Error>>
where
    T: DrawTarget,
{
    pub fn noop(parent: &'a mut T) -> Self {
        Self::new(parent, |_| Ok(()))
    }
}

impl<'a, T, F> Flushable for Flushing<'a, T, F>
where
    T: DrawTarget,
    F: FnMut(&mut T) -> Result<(), T::Error>,
{
    fn flush(&mut self) -> Result<(), Self::Error> {
        let Self {
            parent: target,
            flusher,
        } = self;

        (flusher)(target)
    }
}

impl<'a, T, F> DrawTarget for Flushing<'a, T, F>
where
    T: DrawTarget,
{
    type Error = T::Error;
    type Color = T::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.parent.draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.parent.fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.clear(color)
    }
}

impl<'a, T, F> Dimensions for Flushing<'a, T, F>
where
    T: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        self.parent.bounding_box()
    }
}

//
// BufferedDrawTarget
//

pub struct Buffered<'a, T>
where
    T: DrawTarget,
{
    current: PackedFramebuffer<'a, T::Color>,
    reference: PackedFramebuffer<'a, T::Color>,
    target: T,
}

pub const fn buffer_size<C>(display_size: Size) -> usize
where
    C: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    PackedFramebuffer::<C>::buffer_size(display_size)
}

impl<'a, T> Buffered<'a, T>
where
    T: DrawTarget,
    T::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    pub fn new(draw_buf: &'a mut [u8], reference_buf: &'a mut [u8], display: T) -> Self {
        let bbox = display.bounding_box();

        Self {
            current: PackedFramebuffer::<T::Color>::new(
                draw_buf,
                bbox.size.width as _,
                bbox.size.height as _,
            ),
            reference: PackedFramebuffer::<T::Color>::new(
                reference_buf,
                bbox.size.width as _,
                bbox.size.height as _,
            ),
            target: display,
        }
    }
}

impl<'a, T> Dimensions for Buffered<'a, T>
where
    T: DrawTarget,
    T::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    fn bounding_box(&self) -> Rectangle {
        self.current.bounding_box()
    }
}

impl<'a, T> DrawTarget for Buffered<'a, T>
where
    T: DrawTarget,
    T::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    type Error = T::Error;

    type Color = T::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.current.draw_iter(pixels).unwrap();

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.current.fill_contiguous(area, colors).unwrap();

        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.current.fill_solid(area, color).unwrap();

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.current.clear(color).unwrap();

        Ok(())
    }
}

impl<'a, T> Flushable for Buffered<'a, T>
where
    T: Flushable,
    T::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.reference.apply(&mut self.current, &mut self.target)?;

        self.target.flush()
    }
}

//
// PackedFramebuffer
//

pub struct PackedFramebuffer<'a, COLOR> {
    buf: &'a mut [u8],
    width: usize,
    height: usize,
    _color: PhantomData<COLOR>,
}

impl<'a, COLOR> PackedFramebuffer<'a, COLOR>
where
    COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    const BITS_PER_PIXEL: usize = Self::bits_per_pixel();
    const PIXEL_MASK: u8 = ((1 << Self::BITS_PER_PIXEL) - 1) as u8;
    const PIXELS_PER_BYTE: usize = 8 / Self::BITS_PER_PIXEL;
    const PIXELS_PER_BYTE_SHIFT: usize = if Self::BITS_PER_PIXEL == 8 {
        0
    } else {
        Self::BITS_PER_PIXEL
    };

    pub fn new(buf: &'a mut [u8], width: usize, height: usize) -> Self {
        Self {
            buf,
            width,
            height,
            _color: PhantomData,
        }
    }

    pub const fn buffer_size(display_size: Size) -> usize {
        display_size.width as usize * display_size.height as usize / (8 / Self::bits_per_pixel())
    }

    const fn bits_per_pixel() -> usize {
        if COLOR::Raw::BITS_PER_PIXEL > 4 {
            8
        } else if COLOR::Raw::BITS_PER_PIXEL > 2 {
            4
        } else if COLOR::Raw::BITS_PER_PIXEL > 1 {
            2
        } else {
            1
        }
    }

    pub fn apply<D>(&mut self, new: &Self, to: &mut D) -> Result<usize, D::Error>
    where
        D: DrawTarget<Color = COLOR>,
    {
        let width = self.width();
        let height = self.height();

        let mut changes = 0_usize;

        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter_map(|(x, y)| {
                let bytes_offset = self.y_offset(y as usize) + Self::x_offset(x as usize);
                let bits_offset = Self::x_bits_offset(x as usize);

                let color = new.get(bytes_offset, bits_offset);
                if self.get(bytes_offset, bits_offset) != color {
                    self.set(bytes_offset, bits_offset, color);

                    changes += 1;

                    Some(Pixel(Point::new(x as _, y as _), color))
                } else {
                    None
                }
            });

        to.draw_iter(pixels)?;

        trace!(
            "Display updated ({}/{} changed pixels)",
            changes,
            width * height
        );

        Ok(changes)
    }

    fn offsets(&self, area: Rectangle) -> impl Iterator<Item = (usize, usize)> {
        let dimensions = self.bounding_box();
        let bottom_right = dimensions.bottom_right().unwrap_or(dimensions.top_left);

        let x = min(max(area.top_left.x, 0), bottom_right.x) as usize;
        let y = min(max(area.top_left.y, 0), bottom_right.y) as usize;

        let xend = min(
            max(area.top_left.x + area.size.width as i32, 0),
            bottom_right.x,
        ) as usize;
        let yend = min(
            max(area.top_left.y + area.size.height as i32, 0),
            bottom_right.y,
        ) as usize;

        (self.y_offset(y)..self.y_offset(yend))
            .step_by(self.bytes_per_row())
            .flat_map(move |y_offset| {
                (x..xend).map(move |x| (y_offset + Self::x_offset(x), Self::x_bits_offset(x)))
            })
    }

    #[inline(always)]
    fn width(&self) -> usize {
        self.width
    }

    #[inline(always)]
    fn height(&self) -> usize {
        self.height
    }

    #[inline(always)]
    fn to_bits(color: COLOR) -> u8 {
        color.into_storage()
    }

    #[inline(always)]
    fn from_bits(bits: u8) -> COLOR {
        bits.into()
    }

    #[inline(always)]
    fn y_offset(&self, y: usize) -> usize {
        y * self.bytes_per_row()
    }

    #[inline(always)]
    fn x_offset(x: usize) -> usize {
        x / Self::PIXELS_PER_BYTE
    }

    #[inline(always)]
    fn x_bits_offset(x: usize) -> usize {
        Self::PIXELS_PER_BYTE_SHIFT * (x % Self::PIXELS_PER_BYTE)
    }

    #[inline(always)]
    fn bytes_per_row(&self) -> usize {
        self.width() / Self::PIXELS_PER_BYTE
    }

    #[inline(always)]
    fn get(&self, byte_offset: usize, bits_offset: usize) -> COLOR {
        Self::from_bits((self.buf[byte_offset] >> bits_offset) & Self::PIXEL_MASK)
    }

    #[inline(always)]
    fn set(&mut self, byte_offset: usize, bits_offset: usize, color: COLOR) {
        let byte = &mut self.buf[byte_offset];
        *byte &= !(Self::PIXEL_MASK << bits_offset);
        *byte |= Self::to_bits(color) << bits_offset;
    }
}

impl<'a, COLOR> Dimensions for PackedFramebuffer<'a, COLOR>
where
    COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(
            Point::zero(),
            Size::new(self.width() as u32, self.height() as u32),
        )
    }
}

impl<'a, COLOR> DrawTarget for PackedFramebuffer<'a, COLOR>
where
    COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    type Error = Infallible;

    type Color = COLOR;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels {
            if pixel.0.x >= 0
                && pixel.0.x < self.width() as _
                && pixel.0.y >= 0
                && pixel.0.y < self.height() as _
            {
                self.set(
                    self.y_offset(pixel.0.y as usize) + Self::x_offset(pixel.0.x as usize),
                    Self::x_bits_offset(pixel.0.x as usize),
                    pixel.1,
                );
            }
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut colors = colors.into_iter();

        for (byte_offset, bits_offset) in self.offsets(*area) {
            if let Some(color) = colors.next() {
                self.set(byte_offset, bits_offset, color);
            }
        }

        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        for (byte_offset, bits_offset) in self.offsets(*area) {
            self.set(byte_offset, bits_offset, color);
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        if Self::to_bits(color) == 0 {
            for byte in self.buf.iter_mut() {
                *byte = 0;
            }
        } else {
            for (byte_offset, bits_offset) in self.offsets(self.bounding_box()) {
                self.set(byte_offset, bits_offset, color);
            }
        }

        Ok(())
    }
}

//
// Rotated
//

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RotateAngle {
    Degrees90,
    Degrees180,
    Degrees270,
}

impl RotateAngle {
    fn transform(&self, point: Point, bbox: &Rectangle) -> Point {
        match self {
            RotateAngle::Degrees90 => Point::new(
                point.y,
                bbox.top_left.x * 2 + bbox.size.width as i32 - point.x,
            ),
            RotateAngle::Degrees180 => Point::new(
                bbox.top_left.x * 2 + bbox.size.width as i32 - point.x,
                bbox.top_left.y * 2 + bbox.size.height as i32 - point.y,
            ),
            RotateAngle::Degrees270 => Point::new(
                bbox.top_left.y * 2 + bbox.size.height as i32 - point.y,
                bbox.top_left.x * 2 + bbox.size.width as i32 - point.x,
            ),
        }
    }

    fn transform_rect(&self, rect: &Rectangle, bbox: &Rectangle) -> Rectangle {
        let point1 = self.transform(rect.top_left, bbox);
        let point2 = self.transform(rect.top_left + rect.size, bbox);

        let x1 = min(point1.x, point2.x);
        let y1 = min(point1.y, point2.y);

        let x2 = max(point1.x, point2.x);
        let y2 = max(point1.y, point2.y);

        Rectangle::with_corners(Point::new(x1, y1), Point::new(x2, y2))
    }
}

pub struct Rotated<'a, T>
where
    T: DrawTarget,
{
    parent: &'a mut T,
    angle: RotateAngle,
}

impl<'a, T> Rotated<'a, T>
where
    T: DrawTarget,
{
    pub fn new(parent: &'a mut T, angle: RotateAngle) -> Self {
        Self { parent, angle }
    }
}

impl<'a, T> DrawTarget for Rotated<'a, T>
where
    T: DrawTarget,
{
    type Error = T::Error;
    type Color = T::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bbox = self.parent.bounding_box();
        let angle = self.angle;

        self.parent.draw_iter(
            pixels
                .into_iter()
                .map(|pixel| Pixel(angle.transform(pixel.0, &bbox), pixel.1)),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let bbox = self.parent.bounding_box();
        let angle = self.angle;

        self.parent.draw_iter(
            area.points()
                .zip(colors)
                .map(|(pos, color)| Pixel(angle.transform(pos, &bbox), color)),
        )
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let bbox = self.parent.bounding_box();
        let angle = self.angle;

        self.parent
            .fill_solid(&angle.transform_rect(area, &bbox), color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.clear(color)
    }
}

impl<'a, T> Dimensions for Rotated<'a, T>
where
    T: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        if self.angle != RotateAngle::Degrees180 {
            let bbox = self.parent.bounding_box();

            Rectangle::new(
                Point::new(bbox.top_left.y, bbox.top_left.x),
                Size::new(bbox.size.height, bbox.size.width),
            )
        } else {
            self.parent.bounding_box()
        }
    }
}

//
// Scaled
//

pub struct Scaled<'a, T>
where
    T: DrawTarget,
{
    parent: &'a mut T,
    size: Size,
}

impl<'a, T> Scaled<'a, T>
where
    T: DrawTarget,
{
    pub fn new(parent: &'a mut T, size: Size) -> Self {
        Self { parent, size }
    }

    fn scale(point: Point, size: Size, bbox: &Rectangle) -> Point {
        Point::new(
            (point.x - bbox.top_left.x) * size.width as i32 / bbox.size.width as i32
                + bbox.top_left.x,
            (point.y - bbox.top_left.y) * size.height as i32 / bbox.size.height as i32
                + bbox.top_left.y,
        )
    }
}

impl<'a, T> DrawTarget for Scaled<'a, T>
where
    T: DrawTarget,
{
    type Error = T::Error;
    type Color = T::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bbox = self.parent.bounding_box();
        let size = self.size;

        self.parent.draw_iter(
            pixels
                .into_iter()
                .map(|pixel| Pixel(Self::scale(pixel.0, size, &bbox), pixel.1)),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let bbox = self.parent.bounding_box();
        let size = self.size;

        self.parent.draw_iter(
            area.points()
                .zip(colors)
                .map(|(pos, color)| Pixel(Self::scale(pos, size, &bbox), color)),
        )
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let area = Rectangle::new(area.top_left, self.size);

        self.parent.fill_solid(&area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.clear(color)
    }
}

impl<'a, T> Dimensions for Scaled<'a, T>
where
    T: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(self.parent.bounding_box().top_left, self.size)
    }
}

//
// DrawTargetExt2
//

pub trait DrawTargetExt2: DrawTarget + Sized {
    fn rotated(&mut self, angle: RotateAngle) -> Rotated<'_, Self>;

    fn scaled(&mut self, size: Size) -> Scaled<'_, Self>;

    fn flushing<F: FnMut(&mut Self) -> Result<(), Self::Error>>(
        &mut self,
        flusher: F,
    ) -> Flushing<'_, Self, F>;

    fn noop_flushing(&mut self) -> Flushing<'_, Self, fn(&mut Self) -> Result<(), Self::Error>>;
}

impl<T> DrawTargetExt2 for T
where
    T: DrawTarget,
{
    fn rotated(&mut self, angle: RotateAngle) -> Rotated<'_, Self> {
        Rotated::new(self, angle)
    }

    fn scaled(&mut self, size: Size) -> Scaled<'_, Self> {
        Scaled::new(self, size)
    }

    fn flushing<F: FnMut(&mut Self) -> Result<(), Self::Error>>(
        &mut self,
        flusher: F,
    ) -> Flushing<'_, Self, F> {
        Flushing::new(self, flusher)
    }

    fn noop_flushing(&mut self) -> Flushing<'_, Self, fn(&mut Self) -> Result<(), Self::Error>> {
        Flushing::noop(self)
    }
}

pub trait OwnedDrawTargetExt: DrawTarget + Sized {
    fn owned_translated(self, offset: Point) -> Owned<TranslatedT<Self>>;

    fn owned_cropped(self, area: &Rectangle) -> Owned<CroppedT<Self>>;

    fn owned_clipped(self, area: &Rectangle) -> Owned<ClippedT<Self>>;

    fn owned_color_converted<C>(self) -> Owned<ColorConvertedT<Self, C>>
    where
        C: PixelColor + Into<Self::Color>;

    fn owned_rotated(self, angle: RotateAngle) -> Owned<RotatedT<Self>>;

    fn owned_scaled(self, size: Size) -> Owned<ScaledT<Self>>;

    fn owned_flushing<F: FnMut(&mut Self) -> Result<(), Self::Error> + Send + Clone + 'static>(
        self,
        flusher: F,
    ) -> Owned<FlushingT<Self, F>>
    where
        Self: 'static,
        Self::Error: 'static;

    fn owned_noop_flushing(
        self,
    ) -> Owned<FlushingT<Self, fn(&mut Self) -> Result<(), Self::Error>>>
    where
        Self: 'static,
        Self::Error: 'static;

    fn owned_buffered<'a>(
        self,
        draw_buf: &'a mut [u8],
        reference_buf: &'a mut [u8],
    ) -> Buffered<'a, Self>
    where
        Self::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>;
}

impl<T> OwnedDrawTargetExt for T
where
    T: DrawTarget,
{
    fn owned_translated(self, offset: Point) -> Owned<TranslatedT<Self>> {
        TranslatedT(self, offset).into_owned()
    }

    fn owned_cropped(self, area: &Rectangle) -> Owned<CroppedT<Self>> {
        CroppedT(self, *area).into_owned()
    }

    fn owned_clipped(self, area: &Rectangle) -> Owned<ClippedT<Self>> {
        ClippedT(self, *area).into_owned()
    }

    fn owned_color_converted<C>(self) -> Owned<ColorConvertedT<Self, C>>
    where
        C: PixelColor + Into<Self::Color>,
    {
        ColorConvertedT(self, PhantomData::<C>).into_owned()
    }

    fn owned_rotated(self, angle: RotateAngle) -> Owned<RotatedT<Self>> {
        RotatedT(self, angle).into_owned()
    }

    fn owned_scaled(self, size: Size) -> Owned<ScaledT<Self>> {
        ScaledT(self, size).into_owned()
    }

    fn owned_flushing<F: FnMut(&mut Self) -> Result<(), Self::Error> + Send + Clone + 'static>(
        self,
        flusher: F,
    ) -> Owned<FlushingT<Self, F>>
    where
        Self: 'static,
        Self::Error: 'static,
    {
        FlushingT(self, flusher).into_owned()
    }

    fn owned_noop_flushing(self) -> Owned<FlushingT<Self, fn(&mut Self) -> Result<(), Self::Error>>>
    where
        Self: 'static,
        Self::Error: 'static,
    {
        self.owned_flushing(|_| Ok(()))
    }

    fn owned_buffered<'a>(
        self,
        draw_buf: &'a mut [u8],
        reference_buf: &'a mut [u8],
    ) -> Buffered<'a, Self>
    where
        Self::Color: PixelColor + IntoStorage<Storage = u8> + From<u8>,
    {
        Buffered::new(draw_buf, reference_buf, self)
    }
}
