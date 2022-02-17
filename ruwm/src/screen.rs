use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;

use alloc::boxed::Box;
use embedded_graphics::draw_target::DrawTarget;
use embedded_svc::nonblocking::Unblocker;
use futures::future::{select, Either};
use futures::pin_mut;

use embedded_graphics::prelude::{PixelColor, RgbColor};

use embedded_svc::channel::nonblocking::{Receiver, Sender};

use crate::battery::BatteryState;
use crate::button::ButtonCommand;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;

mod pages;
mod shapes;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Page {
    Battery,
}

#[derive(Clone, Eq, PartialEq)]
pub struct DrawRequest {
    active_page: Page,
    valve_state: Option<ValveState>,
    water_meter_state: WaterMeterState,
    battery_state: BatteryState,
}

pub struct Screen<C, B, VU, WU, BU, D> {
    _color: PhantomData<C>,
    draw_request: DrawRequest,
    button_command: B,
    valve_state_updates: VU,
    water_meter_state_updates: WU,
    battery_meter_state_updates: BU,
    draw_engine: D,
}

impl<C, B, VU, WU, BU, D> Screen<C, B, VU, WU, BU, D>
where
    C: PixelColor + RgbColor,
    B: Receiver<Data = ButtonCommand>,
    VU: Receiver<Data = Option<ValveState>>,
    WU: Receiver<Data = WaterMeterState>,
    BU: Receiver<Data = BatteryState>,
    D: Sender<Data = DrawRequest>,
{
    pub fn new(
        button_command: B,
        valve_state_updates: VU,
        water_meter_state_updates: WU,
        battery_meter_state_updates: BU,
        valve_state: Option<ValveState>,
        water_meter_state: WaterMeterState,
        battery_state: BatteryState,
        draw_engine: D,
    ) -> Self {
        Self {
            _color: PhantomData,
            button_command,
            valve_state_updates,
            water_meter_state_updates,
            battery_meter_state_updates,
            draw_request: DrawRequest {
                active_page: Page::Battery,
                valve_state,
                water_meter_state,
                battery_state,
            },
            draw_engine,
        }
    }

    pub async fn run(&mut self) {
        let command = self.button_command.recv();
        let valve_state_updates = self.valve_state_updates.recv();
        let water_meter_state_updates = self.water_meter_state_updates.recv();
        let battery_meter_state_updates = self.battery_meter_state_updates.recv();

        pin_mut!(command);
        pin_mut!(valve_state_updates);
        pin_mut!(water_meter_state_updates);
        pin_mut!(battery_meter_state_updates);

        let draw_request = match select(
            command,
            select(
                valve_state_updates,
                select(water_meter_state_updates, battery_meter_state_updates),
            ),
        )
        .await
        {
            Either::Left((command, _)) => DrawRequest {
                ..self.draw_request
            },
            Either::Right((Either::Left((valve_state, _)), _)) => DrawRequest {
                valve_state: valve_state.unwrap(),
                ..self.draw_request
            },
            Either::Right((Either::Right((Either::Left((water_meter_state, _)), _)), _)) => {
                DrawRequest {
                    water_meter_state: water_meter_state.unwrap(),
                    ..self.draw_request
                }
            }
            Either::Right((Either::Right((Either::Right((battery_state, _)), _)), _)) => {
                DrawRequest {
                    battery_state: battery_state.unwrap(),
                    ..self.draw_request
                }
            }
        };

        if self.draw_request != draw_request {
            self.draw_request = draw_request.clone();

            self.draw_engine.send(draw_request);
        }
    }
}

enum PageDrawable {
    Battery(pages::battery::Battery),
}

pub struct DrawEngine<U, N, D> {
    _unblocker: PhantomData<U>,
    display: Option<D>,
    page_drawable: Option<PageDrawable>,
    draw_notif: N,
}

impl<U, N, D> DrawEngine<U, N, D>
where
    U: Unblocker,
    N: Receiver<Data = DrawRequest>,
    D: DrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    pub fn new(draw_notif: N, display: D) -> Self {
        Self {
            _unblocker: PhantomData,
            display: Some(display),
            page_drawable: None,
            draw_notif,
        }
    }

    pub async fn run(&mut self) {
        let draw_request = self.draw_notif.recv().await.unwrap();

        let display = mem::replace(&mut self.display, None);
        let page_drawable = mem::replace(&mut self.page_drawable, None);

        let result = U::unblock(Box::new(move || {
            Self::draw(display.unwrap(), page_drawable, draw_request)
        }))
        .await;

        self.display = Some(result.0);
        self.page_drawable = result.1;
    }

    fn draw(
        mut display: D,
        mut page_drawable: Option<PageDrawable>,
        draw_request: DrawRequest,
    ) -> (D, Option<PageDrawable>) {
        loop {
            match draw_request.active_page {
                Page::Battery => {
                    if let Some(PageDrawable::Battery(drawable)) = &mut page_drawable {
                        drawable
                            .draw(&mut display, draw_request.battery_state)
                            .unwrap();
                        break;
                    } else {
                        page_drawable = Some(PageDrawable::Battery(pages::battery::Battery::new()));
                    }
                }
            }
        }

        (display, page_drawable)
    }
}
