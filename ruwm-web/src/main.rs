#![recursion_limit = "1024"]

use std::rc::Rc;

use log::*;

use yew::prelude::*;
use yew_router::prelude::*;

use edge_frame::frame::*;
use edge_frame::redust::*;
use edge_frame::role::*;
use edge_frame::wifi::*;

use crate::battery::*;
use crate::middleware::apply_middleware;
use crate::state::*;
use crate::valve::*;

mod battery;
mod error;
mod middleware;
mod state;
mod valve;
mod ws;

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
fn app() -> Html {
    let store = apply_middleware(use_store(|| Rc::new(AppState::new()))).unwrap();

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
                // <Role<AppState> role={RoleValue::User} projection={AppState::role()}>
                //     <RouteNavItem<Routes> text="Home" route={Routes::Home}/>
                // </Role<AppState>>
                <Role<AppState> role={RoleValue::Admin} projection={AppState::role()}>
                    <RouteNavItem<Routes> text="Home" icon="fa-solid fa-droplet" route={Routes::Home}/>
                    <WifiNavItem<Routes> route={Routes::Wifi}/>
                </Role<AppState>>
            </Nav>
            <Status>
                <Role<AppState> role={RoleValue::User} projection={AppState::role()}>
                    <WifiStatusItem<Routes, AppState> route={Routes::Wifi} projection={AppState::wifi()}/>
                    <RoleLogoutStatusItem<Routes, AppState> auth_status_route={Routes::AuthState} projection={AppState::role()}/>
                </Role<AppState>>
            </Status>
            <Content>
                {
                    match route {
                        Routes::Home => html! {
                            <Role<AppState> role={RoleValue::User} projection={AppState::role()} auth=true>
                                <Valve<AppState> projection={AppState::valve()}/>
                                <Battery<AppState> projection={AppState::battery()}/>
                            </Role<AppState>>
                        },
                        Routes::AuthState => html! {
                            <RoleAuthState<Routes, AppState> home={Some(Routes::Home)} projection={AppState::role()}/>
                        },
                        Routes::Wifi => html! {
                            <Role<AppState> role={RoleValue::Admin} projection={AppState::role()} auth=true>
                                <Wifi<AppState> projection={AppState::wifi()}/>
                            </Role<AppState>>
                        },
                    }
                }
            </Content>
        </Frame>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());

    yew::start_app::<App>();
}
