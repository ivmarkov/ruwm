#![recursion_limit = "1024"]

use std::rc::Rc;

use yew::prelude::*;
use yew_router::prelude::*;

use edge_frame::frame::*;
use edge_frame::middleware::*;
use edge_frame::redust::*;
use edge_frame::role::*;
use edge_frame::wifi::*;

use crate::battery::*;
use crate::middleware::*;
use crate::state::*;
use crate::valve::*;

mod battery;
mod middleware;
mod state;
mod valve;

#[cfg(all(feature = "middleware-ws", feature = "middleware-local"))]
compile_error!("Only one of the features `middleware-ws` and `middleware-local` can be enabled.");

#[cfg(not(any(feature = "middleware-ws", feature = "middleware-local")))]
compile_error!("One of the features `middleware-ws` or `middleware-local` must be enabled.");

#[derive(Debug, Routable, Copy, Clone, PartialEq, Eq, Hash)]
enum Routes {
    #[at("/wifi")]
    Wifi,
    #[at("/authstate")]
    AuthState,
    #[at("/")]
    Home,
}

#[function_component(App)]
pub fn app() -> Html {
    #[cfg(feature = "middleware-ws")]
    let channel = channel("ws");

    #[cfg(feature = "middleware-local")]
    let channel = channel(
        comm::REQUEST_QUEUE.sender().into(),
        comm::EVENT_QUEUE.receiver().into(),
    );

    let store = apply_middleware(
        use_store(|| Rc::new(AppState::new())),
        to_request,
        from_event,
        channel,
    )
    .unwrap();

    html! {
        <ContextProvider<UseStoreHandle<AppState>> context={store.clone()}>
            <BrowserRouter>
                <Switch<Routes> render={Switch::render(render)}/>
            </BrowserRouter>
        </ContextProvider<UseStoreHandle<AppState>>>
    }
}

fn render(route: &Routes) -> Html {
    html! {
        <Frame
            app_title="RUWM"
            app_url="https://github.com/ivmarkov/ruwm">
            <Nav>
                // <Role<AppState> role={RoleDto::User} projection={AppState::role()}>
                //     <RouteNavItem<Routes> text="Home" route={Routes::Home}/>
                // </Role<AppState>>
                <Role<AppState> role={RoleDto::Admin} projection={AppState::role()}>
                    <RouteNavItem<Routes> text="Home" icon="fa-solid fa-droplet" route={Routes::Home}/>
                    <WifiNavItem<Routes> route={Routes::Wifi}/>
                </Role<AppState>>
            </Nav>
            <Status>
                <Role<AppState> role={RoleDto::User} projection={AppState::role()}>
                    <WifiStatusItem<Routes, AppState> route={Routes::Wifi} projection={AppState::wifi()}/>
                    <RoleLogoutStatusItem<Routes, AppState> auth_status_route={Routes::AuthState} projection={AppState::role()}/>
                </Role<AppState>>
            </Status>
            <Content>
                {
                    match route {
                        Routes::Home => html! {
                            <Role<AppState> role={RoleDto::User} projection={AppState::role()} auth=true>
                                <Valve<AppState> projection={AppState::valve()}/>
                                <Battery<AppState> projection={AppState::battery()}/>
                            </Role<AppState>>
                        },
                        Routes::AuthState => html! {
                            <RoleAuthState<Routes, AppState> home={Some(Routes::Home)} projection={AppState::role()}/>
                        },
                        Routes::Wifi => html! {
                            <Role<AppState> role={RoleDto::Admin} projection={AppState::role()} auth=true>
                                <Wifi<AppState> projection={AppState::wifi()}/>
                            </Role<AppState>>
                        },
                    }
                }
            </Content>
        </Frame>
    }
}

#[cfg(feature = "middleware-local")]
pub mod comm {
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel};

    use ruwm::dto::web::*;

    pub(crate) static REQUEST_QUEUE: channel::Channel<CriticalSectionRawMutex, WebRequest, 1> =
        channel::Channel::new();
    pub(crate) static EVENT_QUEUE: channel::Channel<CriticalSectionRawMutex, WebEvent, 1> =
        channel::Channel::new();

    pub fn channel() -> (
        channel::DynamicSender<'static, WebEvent>,
        channel::DynamicReceiver<'static, WebRequest>,
    ) {
        (EVENT_QUEUE.sender().into(), REQUEST_QUEUE.receiver().into())
    }
}
