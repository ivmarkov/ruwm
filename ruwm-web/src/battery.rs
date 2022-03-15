use ruwm::battery::BatteryState;

use yew::prelude::*;
use yew_router::prelude::*;

use crate::redust::{use_selector, Selector, SimpleStore, SimpleStoreAction, Store};

pub type BatteryStore = SimpleStore<BatteryState>;
pub type BatteryAction = SimpleStoreAction<BatteryState>;

#[derive(Properties, Clone, PartialEq)]
pub struct BatteryProps<T: Store>
where
    T::Action: PartialEq,
{
    #[prop_or_default]
    pub app_title: String,

    #[prop_or_default]
    pub app_url: String,

    pub selector: Selector<T, BatteryStore, BatteryAction>,
    // // TODO: Most likely should be state
    // #[prop_or(Role::Admin)]
    // pub active_role: Role,
}

#[function_component(Battery)]
pub fn battery<S: Store>(props: &BatteryProps<S>) -> Html
where
    S::Action: PartialEq,
{
    let battery_state = use_selector(props.selector.clone());

    html! {
        {format!("TODO: {}", battery_state.powered.unwrap_or(false))}
    }
}
