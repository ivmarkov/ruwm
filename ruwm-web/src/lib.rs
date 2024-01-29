#![recursion_limit = "1024"]

use core::fmt::Debug;

use std::rc::Rc;

use log::Level;

use yew::prelude::*;
use yew_router::prelude::*;
use yewdux_middleware::*;

use edge_frame::frame::*;
use edge_frame::middleware::{self, *};
use edge_frame::role::*;
use edge_frame::wifi_setup::*;

use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel};

use ruwm::dto::web::*;

use crate::battery::*;
use crate::valve::*;

mod battery;
mod valve;

static REQUEST_QUEUE: channel::Channel<CriticalSectionRawMutex, WebRequest, 1> =
    channel::Channel::new();
static EVENT_QUEUE: channel::Channel<CriticalSectionRawMutex, WebEvent, 1> =
    channel::Channel::new();

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
    let mcx = use_mcx();

    use_effect_with((), move |_| {
        init_middleware(&mcx, endpoint.as_deref());

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
                                <WifiSetup/>
                            </Role>
                        },
                    }
                }
            </Content>
        </Frame>
    }
}

fn init_middleware(mcx: &MiddlewareContext, endpoint: Option<&str>) {
    // Dispatch WebEvent messages => redispatch as BatteryMsg, ValveMsg, RoleState or WifiConf messages
    mcx.register::<WebEvent, _>(|mcx: &MiddlewareContext, event| {
        match event {
            WebEvent::NoPermissions => unreachable!(),
            WebEvent::AuthenticationFailed => {
                mcx.invoke(RoleState::AuthenticationFailed(Credentials {
                    username: "".into(),
                    password: "".into(),
                }))
            } // TODO
            WebEvent::RoleState(role) => mcx.invoke(RoleState::Role(role)),
            WebEvent::ValveState(valve) => mcx.invoke(ValveMsg(valve)),
            WebEvent::BatteryState(battery) => mcx.invoke(BatteryMsg(battery)),
            WebEvent::WaterMeterState(_) => (), // TODO
        }
    });

    mcx.register(log::<RoleStore, RoleState>(
        MiddlewareContext::store.fuse(role_as_request),
    ));
    mcx.register(log::<WifiConfStore, WifiConf>(MiddlewareContext::store));
    mcx.register(log::<BatteryStore, BatteryMsg>(MiddlewareContext::store));
    mcx.register(log::<ValveStore, ValveMsg>(MiddlewareContext::store));

    if let Some(endpoint) = endpoint {
        let (sender, receiver) =
            middleware::open(endpoint).unwrap_or_else(|_| panic!("Failed to open websocket"));

        // Dispatch WebRequest messages => send to backend
        mcx.register(middleware::send::<WebRequest>(sender));

        // Receive from backend => dispatch WebEvent messages
        middleware::receive::<WebEvent>(mcx, receiver);
    } else {
        let (sender, receiver) = (REQUEST_QUEUE.sender(), EVENT_QUEUE.receiver());

        // Dispatch WebRequest messages => send to backend
        mcx.register(middleware::send_local::<WebRequest>(sender));

        // Receive from backend => dispatch WebEvent messages
        middleware::receive_local::<WebEvent>(mcx, receiver);
    }
}

pub fn local_queue() -> (
    DynamicSender<'static, WebEvent>,
    DynamicReceiver<'static, WebRequest>,
) {
    (EVENT_QUEUE.sender().into(), REQUEST_QUEUE.receiver().into())
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

fn role_as_request(mcx: &MiddlewareContext, msg: RoleState, dispatch: impl MiddlewareDispatch<RoleState>) {
    let request = match &msg {
        RoleState::Authenticating(credentials) => Some(WebRequest::Authenticate(
            credentials.username.as_str().try_into().unwrap(),
            credentials.password.as_str().try_into().unwrap(),
        )),
        RoleState::LoggingOut(_) => Some(WebRequest::Logout),
        _ => None,
    };

    if let Some(request) = request {
        mcx.invoke(request);
    }

    dispatch.invoke(mcx, msg);
}
