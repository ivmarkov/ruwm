use std::ops::Deref;

use yew::prelude::*;

use edge_frame::redust::*;

pub type ValveValue = Option<ruwm::dto::valve::ValveState>;

pub type ValveState = ValueState<ValveValue>;
pub type ValveAction = ValueAction<ValveValue>;

#[derive(Properties, Clone, PartialEq)]
pub struct ValveProps<R: Reducible2> {
    pub projection: Projection<R, ValveState, ValveAction>,
}

#[function_component(Valve)]
pub fn valve<R: Reducible2>(props: &ValveProps<R>) -> Html {
    let valve_store = use_projection(props.projection.clone());

    html! {
        {format!("Valve State: {:?}", valve_store.deref())}
    }
}
