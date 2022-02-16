extern crate alloc;
use alloc::format;

use futures::future::{select, Either};
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mqtt::client::nonblocking::{Client, MessageId, Publish, QoS};
use log::info;

use crate::battery::BatteryState;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;

pub struct MqttSender<M, Q, V, W, B> {
    mqtt: M,
    pubq: Q,
    valve_status: V,
    wm_status: W,
    battery_status: B,
}

impl<M, Q, V, W, B> MqttSender<M, Q, V, W, B> 
where
    M: Publish + Client,
    Q: Sender<Data = MessageId>,
    V: Receiver<Data = Option<ValveState>>,
    W: Receiver<Data = WaterMeterState>,
    B: Receiver<Data = BatteryState>,
{
    pub fn new(
        mqtt: M,
        pubq: Q,
        valve_status: V,
        wm_status: W,
        battery_status: B,
    ) -> Self {
        Self {
            mqtt,
            pubq,
            valve_status,
            wm_status,
            battery_status,
        }
    }

    pub async fn run(&mut self, topic_prefix: impl AsRef<str>) {
        let topic_prefix = topic_prefix.as_ref();

        self.mqtt.subscribe(format!("{}/commands/#", topic_prefix), QoS::AtLeastOnce).await.unwrap();

        let topic_valve = format!("{}/valve", topic_prefix);

        let topic_meter_edges = format!("{}/meter/edges", topic_prefix);
        let topic_meter_armed = format!("{}/meter/armed", topic_prefix);
        let topic_meter_leak = format!("{}/meter/leak", topic_prefix);
    
        let topic_battery_voltage = format!("{}/battery/voltage", topic_prefix);
        let topic_battery_low = format!("{}/battery/low", topic_prefix);
        let topic_battery_charged = format!("{}/battery/charged", topic_prefix);
    
        let topic_powered = format!("{}/powered", topic_prefix);

        loop {
            let (valve_state, wm_state, battery_state) =  {
                let valve = self.valve_status.recv();
                let wm = self.wm_status.recv();
                let battery = self.battery_status.recv();

                pin_mut!(valve);
                pin_mut!(wm);
                pin_mut!(battery);

                match select(valve, select(wm, battery)).await {
                    Either::Left((valve_state, _)) => (Some(valve_state.unwrap()), None, None),
                    Either::Right((Either::Left((wm_state, _)), _)) => (None, Some(wm_state.unwrap()), None),
                    Either::Right((Either::Right((battery_state, _)), _)) => (None, None, Some(battery_state.unwrap())),
                }
            };

            if let Some(valve_state) = valve_state {
                let status = match valve_state {
                    Some(ValveState::Open) => "open",
                    Some(ValveState::Opening) => "opening",
                    Some(ValveState::Closed) => "closed",
                    Some(ValveState::Closing) => "closing",
                    None => "unknown",
                };

                self.publish(&topic_valve, QoS::AtLeastOnce, status.as_bytes()).await;
            }

            if let Some(wm_state) = wm_state {
                if wm_state.prev_edges_count != wm_state.edges_count {
                    let num = wm_state.edges_count.to_le_bytes();
                    let num_slice: &[u8] = &num;

                    self.publish(&topic_meter_edges, QoS::AtLeastOnce, num_slice).await;
                }

                if wm_state.prev_armed != wm_state.armed {
                    self.publish(&topic_meter_armed, QoS::AtLeastOnce, (if wm_state.armed { "true" } else { "false" }).as_bytes()).await;
                }

                if wm_state.prev_leaking != wm_state.leaking {
                    self.publish(&topic_meter_leak, QoS::AtLeastOnce, (if wm_state.armed { "true" } else { "false" }).as_bytes()).await;
                }
            }

            if let Some(battery_state) = battery_state {
                if battery_state.prev_voltage != battery_state.voltage {
                    if let Some(voltage) = battery_state.voltage {
                        let num = voltage.to_le_bytes();
                        let num_slice: &[u8] = &num;

                        self.publish(&topic_battery_voltage, QoS::AtMostOnce, num_slice).await;

                        if let Some(prev_voltage) = battery_state.prev_voltage {
                            if (prev_voltage > BatteryState::LOW_VOLTAGE)
                                != (voltage > BatteryState::LOW_VOLTAGE)
                            {
                                let status = if voltage > BatteryState::LOW_VOLTAGE {
                                    "false"
                                } else {
                                    "true"
                                };

                                self.publish(&topic_battery_low, QoS::AtLeastOnce, status.as_bytes()).await;
                            }

                            if (prev_voltage >= BatteryState::MAX_VOLTAGE)
                                != (voltage >= BatteryState::MAX_VOLTAGE)
                            {
                                let status = if voltage >= BatteryState::MAX_VOLTAGE {
                                    "true"
                                } else {
                                    "false"
                                };

                                self.publish(&topic_battery_charged, QoS::AtMostOnce, status.as_bytes()).await;
                            }
                        }
                    }
                }

                if battery_state.prev_powered != battery_state.powered {
                    if let Some(powered) = battery_state.powered {
                        self.publish(
                            &topic_powered,
                            QoS::AtMostOnce,
                            (if powered { "true" } else { "false" }).as_bytes(),
                        )
                        .await;
                    }
                }
            };
        }
    }

    async fn publish(&mut self, topic: &str, qos: QoS, payload: &[u8]) {
        let msg_id = self.mqtt.publish(topic, qos, false, payload)
            .await
            .unwrap();

        info!("Published to {}", topic);

        if qos >= QoS::AtLeastOnce {
            self.pubq.send(msg_id).await.unwrap();
        }
    }
}
