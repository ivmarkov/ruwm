#![recursion_limit = "1024"]

use yew::prelude::*;
use yew_router::prelude::*;

use embedded_svc::utils::rest::role::Role;

use edge_frame::exit;
use edge_frame::frame;
use edge_frame::plugin;
use edge_frame::wifi;

#[derive(Debug, Routable, Copy, Clone, PartialEq, Eq, Hash)]
enum Routes {
    #[at("/exit")]
    Exit,
    #[at("/")]
    Root,
}

#[function_component(App)]
fn app() -> Html {
    wasm_logger::init(wasm_logger::Config::default());

    let wifi = wifi::plugin(wifi::PluginBehavior::Mixed).map(Routes::Root);
    let exit = exit::plugin().map(Routes::Exit);

    let nav = wifi.iter().chain(exit.iter()).collect::<Vec<_>>();
    let content = std::vec![
        plugin::ContentPlugin::from(&wifi),
        plugin::ContentPlugin::from(&exit)
    ];

    html! {
        <frame::Frame<Routes>
            app_title = "RUWM"
            app_url="https://github.com/ivmarkov/ruwm"
            active_role={Role::Admin}
            api_endpoint={None}
            navigation={nav}
            content={content}
            />
    }
}

fn main() {
    yew::start_app::<App>();
}
