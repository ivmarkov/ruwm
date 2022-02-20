use core::marker::PhantomData;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::prelude::{Dimensions, OriginDimensions, PixelColor, Point, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::Pixel;

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

pub struct TransformingAdaptor<T, D> {
    transform: T,
    display: D,
}

impl<T, D> TransformingAdaptor<T, D> {
    pub fn new(transform: T, display: D) -> Self {
        Self { transform, display }
    }
}

impl<T, D> DrawTarget for TransformingAdaptor<T, D>
where
    T: Fn(Point) -> Point,
    D: DrawTarget,
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

impl<T, D> Dimensions for TransformingAdaptor<T, D>
where
    T: Fn(Point) -> Point,
    D: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        let rect = self.display.bounding_box();

        let p1 = (self.transform)(rect.top_left);
        let p2 = (self.transform)(Point::new(
            rect.top_left.x + rect.size.width as i32,
            rect.top_left.y + rect.size.height as i32,
        ));

        let p1 = Point::new(i32::min(p1.x, p2.x), i32::min(p1.y, p2.y));
        let p2 = Point::new(i32::max(p1.x, p2.x), i32::max(p1.y, p2.y));

        Rectangle::new(
            p1,
            Size::new(p2.x as u32 - p1.x as u32, p2.y as u32 - p1.y as u32),
        )
    }
}

pub enum RotateAngle {
    Degrees90(Size),
    Degrees180(Size),
    Degrees270(Size),
}

impl RotateAngle {
    pub fn rotate(&self, point: Point) -> Point {
        match self {
            Self::Degrees90(Size { width: _, height }) => {
                Point::new(*height as i32 - point.y, point.x)
            }
            Self::Degrees180(Size { width, height }) => {
                Point::new(*width as i32 - point.x, *height as i32 - point.y)
            }
            Self::Degrees270(Size { width, height }) => {
                Point::new(*height as i32 - point.y, *width as i32 - point.x)
            }
        }
    }
}

pub struct ScaleDown(Size);

impl ScaleDown {
    pub fn scale_down(&self, point: Point) -> Point {
        Point::new(
            point.x / self.0.width as i32,
            point.y / self.0.height as i32,
        )
    }
}

pub struct Mirror(u32);

impl Mirror {
    pub fn mirror(&self, point: Point) -> Point {
        Point::new(self.0 as i32 - point.x, point.y)
    }
}
