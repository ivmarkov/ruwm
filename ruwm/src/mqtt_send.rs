use alloc::format;

use futures::future::{select, Either};
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mqtt::client::nonblocking::{Client, MessageId, Publish, QoS};

use crate::battery::BatteryState;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;

pub fn subscribe<C>(mqttc: &mut C, topic_prefix: impl AsRef<str>)
where
    C: Client,
{
    mqttc
        .subscribe(format!("{}/#", topic_prefix.as_ref()), QoS::AtLeastOnce)
        .unwrap();
}

pub async fn run<M, Q, V, W, B>(
    mut mqtt: M,
    mut pubq: Q,
    topic_prefix: impl AsRef<str>,
    mut valve_status: V,
    mut wm_status: W,
    mut battery_status: B,
) where
    M: Publish,
    Q: Sender<Data = MessageId>,
    V: Receiver<Data = Option<ValveState>>,
    W: Receiver<Data = WaterMeterState>,
    B: Receiver<Data = BatteryState>,
{
    let topic_prefix = topic_prefix.as_ref();

    let topic_valve = format!("{}/valve", topic_prefix);

    let topic_meter_edges = format!("{}/meter/edges", topic_prefix);
    let topic_meter_armed = format!("{}/meter/armed", topic_prefix);
    let topic_meter_leak = format!("{}/meter/leak", topic_prefix);

    let topic_battery_voltage = format!("{}/battery/voltage", topic_prefix);
    let topic_battery_low = format!("{}/battery/low", topic_prefix);
    let topic_battery_charged = format!("{}/battery/charged", topic_prefix);

    let topic_powered = format!("{}/powered", topic_prefix);

    loop {
        let valve = valve_status.recv();
        let wm = wm_status.recv();
        let battery = battery_status.recv();

        pin_mut!(valve);
        pin_mut!(wm);
        pin_mut!(battery);

        match select(valve, select(wm, battery)).await {
            Either::Left((valve_state, _)) => {
                let valve_state = valve_state.unwrap();

                let status = match valve_state {
                    Some(ValveState::Open) => "open",
                    Some(ValveState::Opening) => "opening",
                    Some(ValveState::Closed) => "closed",
                    Some(ValveState::Closing) => "closing",
                    None => "unknown",
                };

                let msg_id = mqtt
                    .publish(&topic_valve, QoS::AtLeastOnce, false, status.as_bytes())
                    .await
                    .unwrap();

                pubq.send(msg_id).await.unwrap();
            }
            Either::Right((Either::Left((wm_state, _)), _)) => {
                let wm_state = wm_state.unwrap();

                if wm_state.prev_edges_count != wm_state.edges_count {
                    let num = wm_state.edges_count.to_le_bytes();
                    let num_slice: &[u8] = &num;

                    let msg_id = mqtt
                        .publish(&topic_meter_edges, QoS::AtLeastOnce, false, num_slice)
                        .await
                        .unwrap();

                    pubq.send(msg_id).await.unwrap();
                }

                if wm_state.prev_armed != wm_state.armed {
                    let msg_id = mqtt
                        .publish(
                            &topic_meter_armed,
                            QoS::AtLeastOnce,
                            false,
                            (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                        )
                        .await
                        .unwrap();

                    pubq.send(msg_id).await.unwrap();
                }

                if wm_state.prev_leaking != wm_state.leaking {
                    let msg_id = mqtt
                        .publish(
                            &topic_meter_leak,
                            QoS::AtLeastOnce,
                            false,
                            (if wm_state.armed { "true" } else { "false" }).as_bytes(),
                        )
                        .await
                        .unwrap();

                    pubq.send(msg_id).await.unwrap();
                }
            }
            Either::Right((Either::Right((battery_state, _)), _)) => {
                let battery_state = battery_state.unwrap();

                if battery_state.prev_voltage != battery_state.voltage {
                    if let Some(voltage) = battery_state.voltage {
                        let num = voltage.to_le_bytes();
                        let num_slice: &[u8] = &num;

                        mqtt.publish(&topic_battery_voltage, QoS::AtMostOnce, false, num_slice)
                            .await
                            .unwrap();

                        if let Some(prev_voltage) = battery_state.prev_voltage {
                            if (prev_voltage > BatteryState::LOW_VOLTAGE)
                                != (voltage > BatteryState::LOW_VOLTAGE)
                            {
                                let msg_id = mqtt
                                    .publish(
                                        &topic_battery_low,
                                        QoS::AtLeastOnce,
                                        false,
                                        if voltage > BatteryState::LOW_VOLTAGE {
                                            "false"
                                        } else {
                                            "true"
                                        }
                                        .as_bytes(),
                                    )
                                    .await
                                    .unwrap();

                                pubq.send(msg_id).await.unwrap();
                            }

                            if (prev_voltage >= BatteryState::MAX_VOLTAGE)
                                != (voltage >= BatteryState::MAX_VOLTAGE)
                            {
                                mqtt.publish(
                                    &topic_battery_charged,
                                    QoS::AtMostOnce,
                                    false,
                                    if voltage >= BatteryState::MAX_VOLTAGE {
                                        "true"
                                    } else {
                                        "false"
                                    }
                                    .as_bytes(),
                                )
                                .await
                                .unwrap();
                            }
                        }
                    }
                }

                if battery_state.prev_powered != battery_state.powered {
                    if let Some(powered) = battery_state.powered {
                        mqtt.publish(
                            &topic_powered,
                            QoS::AtMostOnce,
                            false,
                            (if powered { "true" } else { "false" }).as_bytes(),
                        )
                        .await
                        .unwrap();
                    }
                }
            }
        };
    }
}
