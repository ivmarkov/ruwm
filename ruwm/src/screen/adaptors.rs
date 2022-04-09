use core::cmp::{max, min};
use core::convert::Infallible;
use core::marker::PhantomData;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::prelude::{
    Dimensions, IntoStorage, OriginDimensions, PixelColor, Point, RawData, Size,
};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::Pixel;

pub struct DrawTargetRef<'a, D>(&'a mut D);

impl<'a, D> DrawTargetRef<'a, D> {
    pub fn new(draw_target: &'a mut D) -> Self {
        Self(draw_target)
    }
}

impl<'a, D> DrawTarget for DrawTargetRef<'a, D>
where
    D: DrawTarget,
{
    type Color = D::Color;

    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.0.draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.0.fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.0.fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.0.clear(color)
    }
}

impl<'a, D> Dimensions for DrawTargetRef<'a, D>
where
    D: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        self.0.bounding_box()
    }
}

pub struct Diff<const N: usize>([Option<Rectangle>; N]);

impl<const N: usize> Diff<N> {
    pub fn empty() -> Self {
        Self([None; N])
    }

    pub fn diff<'a, const WIDTH: usize, COLOR>(
        buf1: &PackedBuffer<'a, WIDTH, COLOR>,
        buf2: &PackedBuffer<'a, WIDTH, COLOR>,
    ) -> Self
    where
        COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
    {
        let mut list = Self::empty();

        let mut add = |diff_start, y_offset, x| {
            if let Some(diff_start) = diff_start {
                list.add(Rectangle::new(
                    Point::new(
                        diff_start as i32,
                        (y_offset / PackedBuffer::<'a, WIDTH, COLOR>::BYTES_PER_ROW) as i32,
                    ),
                    Size::new(x as u32 - diff_start as u32, 1),
                ));
            }
        };

        for y_offset in 0..PackedBuffer::<'a, WIDTH, COLOR>::y_offset(buf1.height()) {
            let mut diff_start = None;

            for x in 0..WIDTH {
                let byte_offset = y_offset + PackedBuffer::<'a, WIDTH, COLOR>::x_offset(x);
                let bits_offset = PackedBuffer::<'a, WIDTH, COLOR>::x_bits_offset(x);

                let color1 = buf1.get(byte_offset, bits_offset);
                let color2 = buf2.get(byte_offset, bits_offset);

                let diff = color1 != color2;

                if diff_start.is_some() != diff {
                    add(diff_start, y_offset, x);
                    diff_start = if diff { Some(x) } else { None };
                }
            }

            add(diff_start, y_offset, WIDTH);
        }

        list
    }

    pub fn add(&mut self, area: Rectangle) {
        let face = Self::face(&area);

        let closest = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, area)| area.map(|area| (index, area)))
            .map(|(index, carea)| {
                let unioned = Self::unioned(&area, &carea);
                let ratio = (face + Self::face(&carea)) * 100 / Self::face(&unioned);

                (index, unioned, ratio)
            })
            .min_by(|(_, _, ratio1), (_, _, ratio2)| ratio1.cmp(&ratio2));

        let placeholder =
            self.0
                .iter_mut()
                .find_map(|area| if area.is_some() { None } else { Some(area) });

        if let Some((index, unioned, ratio)) = closest {
            if ratio > 80 || placeholder.is_none() {
                self.0[index] = None;
                return self.add(unioned);
            }
        }

        *placeholder.unwrap() = Some(area);
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Rectangle> {
        self.0.iter().filter_map(|area| area.as_ref())
    }

    #[inline(always)]
    fn unioned(area1: &Rectangle, area2: &Rectangle) -> Rectangle {
        let x1 = min(area1.top_left.x, area2.top_left.x);
        let y1 = min(area1.top_left.y, area2.top_left.y);
        let x2 = max(
            area1.top_left.x + area1.size.width as i32,
            area2.top_left.x + area2.size.width as i32,
        );
        let y2 = max(
            area1.top_left.y + area1.size.height as i32,
            area2.top_left.y + area2.size.height as i32,
        );

        Rectangle::new(
            Point::new(x1, y1),
            Size::new((x2 - x1) as u32, (y2 - y1) as u32),
        )
    }

    #[inline(always)]
    fn face(area: &Rectangle) -> u32 {
        area.size.width * area.size.height
    }
}

pub struct PackedBuffer<'a, const WIDTH: usize, COLOR>(&'a mut [u8], PhantomData<COLOR>);

impl<'a, const WIDTH: usize, COLOR> PackedBuffer<'a, WIDTH, COLOR>
where
    COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    const BITS_PER_PIXEL: usize = if COLOR::Raw::BITS_PER_PIXEL > 4 {
        8
    } else if COLOR::Raw::BITS_PER_PIXEL > 2 {
        4
    } else if COLOR::Raw::BITS_PER_PIXEL > 1 {
        2
    } else {
        1
    };
    const PIXEL_MASK: u8 = ((1 << Self::BITS_PER_PIXEL) - 1) as u8;
    const PIXELS_PER_BYTE: usize = 8 / Self::BITS_PER_PIXEL;
    const PIXELS_PER_BYTE_SHIFT: usize = 8 / Self::BITS_PER_PIXEL;
    const BYTES_PER_ROW: usize = WIDTH / Self::PIXELS_PER_BYTE;

    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self(buffer, PhantomData)
    }

    pub fn apply<D>(
        &self,
        diffs: impl Iterator<Item = Rectangle>,
        to: &mut D,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = COLOR>,
    {
        for area in diffs {
            to.fill_contiguous(
                &area,
                Self::offsets(area)
                    .map(|(byte_offset, bits_offset)| self.get(byte_offset, bits_offset)),
            )?;
        }

        Ok(())
    }

    fn offsets(area: Rectangle) -> impl Iterator<Item = (usize, usize)> {
        (Self::y_offset(area.top_left.y as usize)
            ..Self::y_offset(area.top_left.y as usize + area.size.height as usize))
            .step_by(Self::BYTES_PER_ROW)
            .flat_map(move |y_offset| {
                (area.top_left.x as usize..area.top_left.x as usize + area.size.width as usize)
                    .map(move |x| (y_offset + Self::x_offset(x), Self::x_bits_offset(x)))
            })
    }

    #[inline(always)]
    fn height(&self) -> usize {
        self.0.len() / Self::BYTES_PER_ROW
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
    fn y_offset(y: usize) -> usize {
        y * Self::BYTES_PER_ROW
    }

    #[inline(always)]
    fn x_offset(x: usize) -> usize {
        x >> Self::PIXELS_PER_BYTE_SHIFT
    }

    #[inline(always)]
    fn x_bits_offset(x: usize) -> usize {
        x - (Self::x_offset(x) << Self::PIXELS_PER_BYTE_SHIFT)
    }

    #[inline(always)]
    fn get(&self, byte_offset: usize, bits_offset: usize) -> COLOR {
        Self::from_bits((self.0[byte_offset] >> bits_offset) & Self::PIXEL_MASK)
    }

    #[inline(always)]
    fn set(&mut self, byte_offset: usize, bits_offset: usize, color: COLOR) {
        let byte = &mut self.0[byte_offset];
        *byte &= !(Self::PIXEL_MASK << bits_offset);
        *byte |= Self::to_bits(color) << bits_offset;
    }
}

impl<'a, const W: usize, COLOR> Dimensions for PackedBuffer<'a, W, COLOR>
where
    COLOR: PixelColor + IntoStorage<Storage = u8> + From<u8>,
{
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), Size::new(W as u32, self.height() as u32))
    }
}

impl<'a, const W: usize, COLOR> DrawTarget for PackedBuffer<'a, W, COLOR>
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
            self.set(
                Self::y_offset(pixel.0.y as usize) + Self::x_offset(pixel.0.x as usize),
                Self::x_bits_offset(pixel.0.x as usize),
                pixel.1,
            );
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut colors = colors.into_iter();

        for (byte_offset, bits_offset) in Self::offsets(area.clone()) {
            if let Some(color) = colors.next() {
                self.set(byte_offset, bits_offset, color);
            }
        }

        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        for (byte_offset, bits_offset) in Self::offsets(area.clone()) {
            self.set(byte_offset, bits_offset, color);
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        if Self::to_bits(color) == 0 {
            for byte in self.0.iter_mut() {
                *byte = 0;
            }
        } else {
            for (byte_offset, bits_offset) in Self::offsets(self.bounding_box().clone()) {
                self.set(byte_offset, bits_offset, color);
            }
        }

        Ok(())
    }
}

pub trait FlushableDrawTarget: DrawTarget {
    fn flush(&mut self) -> Result<(), Self::Error>;
}

pub struct FlushableAdaptor<A, D> {
    adaptor: A,
    display: D,
}

impl<A, D> FlushableAdaptor<A, D> {
    pub fn new(adaptor: A, display: D) -> Self {
        Self { adaptor, display }
    }
}

impl<D> FlushableAdaptor<fn(&mut D) -> Result<(), D::Error>, D>
where
    D: DrawTarget,
{
    pub fn noop(display: D) -> Self {
        Self {
            adaptor: |_| Result::<_, D::Error>::Ok(()),
            display,
        }
    }
}

impl<A, D> FlushableDrawTarget for FlushableAdaptor<A, D>
where
    A: Fn(&mut D) -> Result<(), D::Error>,
    D: DrawTarget,
{
    fn flush(&mut self) -> Result<(), Self::Error> {
        (self.adaptor)(&mut self.display)
    }
}

impl<A, D> DrawTarget for FlushableAdaptor<A, D>
where
    D: DrawTarget,
{
    type Error = D::Error;

    type Color = D::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.display.draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.display.fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.display.fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.display.clear(color)
    }
}

impl<A, D> Dimensions for FlushableAdaptor<A, D>
where
    D: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        self.display.bounding_box()
    }
}

pub struct CroppedAdaptor<D> {
    draw_area: Rectangle,
    display: D,
}

impl<D> CroppedAdaptor<D> {
    pub fn new(draw_area: Rectangle, display: D) -> Self {
        Self { draw_area, display }
    }
}

impl<D> DrawTarget for CroppedAdaptor<D>
where
    D: DrawTarget,
{
    type Error = D::Error;

    type Color = D::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.display.cropped(&self.draw_area).draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.display
            .cropped(&self.draw_area)
            .fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.display
            .cropped(&self.draw_area)
            .fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.display.cropped(&self.draw_area).clear(color)
    }
}

impl<D> OriginDimensions for CroppedAdaptor<D> {
    fn size(&self) -> Size {
        self.draw_area.size
    }
}

pub struct ColorAdaptor<C, A, D> {
    _color: PhantomData<C>,
    adaptor: A,
    display: D,
}

impl<C, A, D> ColorAdaptor<C, A, D>
where
    A: Fn(C) -> D::Color,
    C: PixelColor,
    D: DrawTarget,
{
    pub fn new(adaptor: A, display: D) -> Self {
        Self {
            _color: PhantomData,
            adaptor,
            display,
        }
    }
}

impl<C, A, D> DrawTarget for ColorAdaptor<C, A, D>
where
    A: Fn(C) -> D::Color,
    C: PixelColor,
    D: DrawTarget,
{
    type Error = D::Error;

    type Color = C;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let display = &mut self.display;
        let adaptor = &self.adaptor;

        display.draw_iter(
            pixels
                .into_iter()
                .map(|pixel| Pixel(pixel.0, (adaptor)(pixel.1))),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let display = &mut self.display;
        let adaptor = &self.adaptor;

        display.fill_contiguous(area, colors.into_iter().map(adaptor))
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.display.fill_solid(area, (self.adaptor)(color))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.display.clear((self.adaptor)(color))
    }
}

impl<C, A, D> Dimensions for ColorAdaptor<C, A, D>
where
    D: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        self.display.bounding_box()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RotateAngle {
    Degrees90,
    Degrees180,
    Degrees270,
}

pub struct TransformingAdaptor<D, T> {
    display: D,
    transform: T,
}

impl<D> TransformingAdaptor<D, fn(Point) -> Point> {
    pub fn display(display: D) -> Self {
        Self::new(display, core::convert::identity)
    }
}

impl<D, T> TransformingAdaptor<D, T> {
    pub fn new(display: D, transform: T) -> Self {
        Self { display, transform }
    }

    pub fn translate(self, to: Point) -> TransformingAdaptor<Self, impl Fn(Point) -> Point>
    where
        D: DrawTarget + Dimensions,
        T: Fn(Point) -> Point,
    {
        TransformingAdaptor::new(self, move |point: Point| {
            Point::new(point.x + to.x, point.y + to.y)
        })
    }

    pub fn rotate(self, angle: RotateAngle) -> TransformingAdaptor<Self, impl Fn(Point) -> Point>
    where
        D: DrawTarget + Dimensions,
        T: Fn(Point) -> Point,
    {
        let bbox = self.transform_rect(&self.display.bounding_box());

        TransformingAdaptor::new(self, move |point: Point| match angle {
            RotateAngle::Degrees90 => Point::new(
                bbox.top_left.y + bbox.size.height as i32 - point.y - 1,
                point.x,
            ),
            RotateAngle::Degrees180 => Point::new(
                bbox.top_left.x + bbox.size.width as i32 - point.x - 1,
                bbox.top_left.y + bbox.size.height as i32 - point.y - 1,
            ),
            RotateAngle::Degrees270 => Point::new(
                point.y,
                bbox.top_left.x + bbox.size.width as i32 - point.x - 1,
            ),
        })
    }

    pub fn mirror(self, horizontal: bool) -> TransformingAdaptor<Self, impl Fn(Point) -> Point>
    where
        D: DrawTarget + Dimensions,
        T: Fn(Point) -> Point,
    {
        let bbox = self.transform_rect(&self.display.bounding_box());

        TransformingAdaptor::new(self, move |point: Point| {
            Point::new(
                if horizontal {
                    bbox.top_left.x + bbox.size.width as i32 - point.x - 1
                } else {
                    point.x
                },
                if horizontal {
                    point.y
                } else {
                    bbox.top_left.y + bbox.size.height as i32 - point.y - 1
                },
            )
        })
    }

    pub fn scale(self, to: Size) -> TransformingAdaptor<Self, impl Fn(Point) -> Point>
    where
        D: DrawTarget + Dimensions,
        T: Fn(Point) -> Point,
    {
        let bbox = self.transform_rect(&self.display.bounding_box());

        self.scale_from(bbox.size, to)
    }

    pub fn scale_from(
        self,
        from: Size,
        to: Size,
    ) -> TransformingAdaptor<Self, impl Fn(Point) -> Point>
    where
        D: DrawTarget + Dimensions,
        T: Fn(Point) -> Point,
    {
        let bbox = self.transform_rect(&self.display.bounding_box());

        TransformingAdaptor::new(self, move |point: Point| {
            Point::new(
                bbox.top_left.x + (point.x - bbox.top_left.x) * to.width as i32 / from.width as i32,
                bbox.top_left.y
                    + (point.y - bbox.top_left.y) * to.height as i32 / from.height as i32,
            )
        })
    }

    fn transform_rect(&self, rect: &Rectangle) -> Rectangle
    where
        T: Fn(Point) -> Point,
    {
        let p1 = (self.transform)(rect.top_left);
        let p2 = (self.transform)(Point::new(
            rect.top_left.x + rect.size.width as i32,
            rect.top_left.y + rect.size.height as i32,
        ));

        let p1f = Point::new(i32::min(p1.x, p2.x), i32::min(p1.y, p2.y));
        let p2f = Point::new(i32::max(p1.x, p2.x), i32::max(p1.y, p2.y));

        Rectangle::new(
            p1f,
            Size::new((p2f.x - p1f.x) as u32, (p2f.y - p1f.y) as u32),
        )
    }
}

impl<D, T> DrawTarget for TransformingAdaptor<D, T>
where
    D: DrawTarget,
    T: Fn(Point) -> Point,
{
    type Error = D::Error;

    type Color = D::Color;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let display = &mut self.display;
        let transform = &self.transform;

        display.draw_iter(
            pixels
                .into_iter()
                .map(|pixel| Pixel((transform)(pixel.0), pixel.1)),
        )
    }

    #[allow(unconditional_recursion)]
    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        DrawTarget::fill_contiguous(self, area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.display.fill_solid(&self.transform_rect(area), color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.display.clear(color)
    }
}

impl<D, T> Dimensions for TransformingAdaptor<D, T>
where
    D: Dimensions,
    T: Fn(Point) -> Point,
{
    fn bounding_box(&self) -> Rectangle {
        self.display.bounding_box()
    }
}
