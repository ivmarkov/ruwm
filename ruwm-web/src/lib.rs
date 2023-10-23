#![recursion_limit = "1024"]

use core::fmt::Debug;

use std::rc::Rc;

use edge_frame::middleware;
use log::Level;
use ruwm::dto::web::WebEvent;
use ruwm::dto::web::WebRequest;
use yew::prelude::*;
use yew_router::prelude::*;
use yewdux_middleware::*;

use edge_frame::frame::*;
use edge_frame::middleware::*;
use edge_frame::role::*;
use edge_frame::wifi::*;

use crate::battery::*;
use crate::valve::*;

mod battery;
mod valve;

#[derive(Debug, Routable, Copy, Clone, PartialEq, Eq, Hash)]
enum Routes {
    #[at("/wifi")]
    Wifi,
    #[at("/authstate")]
    AuthState,
    #[at("/")]
    Home,
}

#[derive(Default, Properties, Clone, PartialEq)]
pub struct AppProps {
    #[prop_or_default]
    pub endpoint: Option<String>,
}

#[function_component(App)]
pub fn app(props: &AppProps) -> Html {
    let endpoint = props.endpoint.clone();

    use_effect_with((), move |_| {
        init_middleware(endpoint.as_deref());

        move || ()
    });

    html! {
        <BrowserRouter>
            <Switch<Routes> render={render}/>
        </BrowserRouter>
    }
}

fn render(route: Routes) -> Html {
    html! {
        <Frame
            app_title="RUWM"
            app_url="https://github.com/ivmarkov/ruwm">
            <Nav>
                // <Role role={RoleDto::User}>
                //     <RouteNavItem text="Home" route={Routes::Home}/>
                // </Role>
                <Role role={RoleDto::Admin}>
                    <RouteNavItem<Routes> text="Home" icon="fa-solid fa-droplet" route={Routes::Home}/>
                    <WifiNavItem<Routes> route={Routes::Wifi}/>
                </Role>
            </Nav>
            <Status>
                <Role role={RoleDto::User}>
                    <WifiStatusItem<Routes> route={Routes::Wifi}/>
                    <RoleLogoutStatusItem<Routes> auth_status_route={Routes::AuthState}/>
                </Role>
            </Status>
            <Content>
                {
                    match route {
                        Routes::Home => html! {
                            <Role role={RoleDto::User} auth=true>
                                <Valve/>
                                <Battery/>
                            </Role>
                        },
                        Routes::AuthState => html! {
                            <RoleAuthState<Routes> home={Some(Routes::Home)}/>
                        },
                        Routes::Wifi => html! {
                            <Role role={RoleDto::Admin} auth=true>
                                <Wifi/>
                            </Role>
                        },
                    }
                }
            </Content>
        </Frame>
    }
}

fn init_middleware(endpoint: Option<&str>) {
    // Dispatch WebEvent messages => redispatch as BatteryMsg, ValveMsg, RoleState or WifiConf messages
    dispatch::register::<WebEvent, _>(|event| {
        match event {
            WebEvent::NoPermissions => unreachable!(),
            WebEvent::AuthenticationFailed => {
                dispatch::invoke(RoleState::AuthenticationFailed(Credentials {
                    username: "".into(),
                    password: "".into(),
                }))
            } // TODO
            WebEvent::RoleState(role) => dispatch::invoke(RoleState::Role(role)),
            WebEvent::ValveState(valve) => dispatch::invoke(ValveMsg(valve)),
            WebEvent::BatteryState(battery) => dispatch::invoke(BatteryMsg(battery)),
            WebEvent::WaterMeterState(_) => (), // TODO
        }
    });

    dispatch::register(log::<RoleStore, RoleState>(
        dispatch::store.fuse(role_as_request),
    ));
    dispatch::register(log::<WifiConfStore, WifiConfState>(dispatch::store));
    dispatch::register(log::<BatteryStore, BatteryMsg>(dispatch::store));
    dispatch::register(log::<ValveStore, ValveMsg>(dispatch::store));

    if let Some(endpoint) = endpoint {
        let (sender, receiver) =
            middleware::open(endpoint).unwrap_or_else(|_| panic!("Failed to open websocket"));

        // Dispatch WebRequest messages => send to backend
        dispatch::register(middleware::send::<WebRequest>(sender));

        // Receive from backend => dispatch WebEvent messages
        middleware::receive::<WebEvent>(receiver);
    } else {
        let (sender, receiver) = (comm::REQUEST_QUEUE.sender(), comm::EVENT_QUEUE.receiver());

        // Dispatch WebRequest messages => send to backend
        dispatch::register(middleware::send_local::<WebRequest>(sender));

        // Receive from backend => dispatch WebEvent messages
        middleware::receive_local::<WebEvent>(receiver);
    }
}

fn log<S, M>(dispatch: impl MiddlewareDispatch<M> + Clone) -> impl MiddlewareDispatch<M>
where
    S: Store + Debug,
    M: Reducer<S> + Debug + 'static,
{
    dispatch
        .fuse(Rc::new(log_store(Level::Trace)))
        .fuse(Rc::new(log_msg(Level::Trace)))
}

fn role_as_request(msg: RoleState, dispatch: impl MiddlewareDispatch<RoleState>) {
    let request = match &msg {
        RoleState::Authenticating(credentials) => Some(WebRequest::Authenticate(
            credentials.username.as_str().into(),
            credentials.password.as_str().into(),
        )),
        RoleState::LoggingOut(_) => Some(WebRequest::Logout),
        _ => None,
    };

    if let Some(request) = request {
        dispatch::invoke(request);
    }

    dispatch.invoke(msg);
}

pub mod comm {
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel};

    use ruwm::dto::web::*;

    pub(crate) static REQUEST_QUEUE: channel::Channel<CriticalSectionRawMutex, WebRequest, 1> =
        channel::Channel::new();
    pub(crate) static EVENT_QUEUE: channel::Channel<CriticalSectionRawMutex, WebEvent, 1> =
        channel::Channel::new();

    pub fn sender() -> channel::DynamicSender<'static, WebEvent> {
        EVENT_QUEUE.sender().into()
    }

    pub fn receiver() -> channel::DynamicReceiver<'static, WebRequest> {
        REQUEST_QUEUE.receiver().into()
    }
}
