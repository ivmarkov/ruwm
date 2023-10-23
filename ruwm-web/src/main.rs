fn main() {
    wasm_logger::init(wasm_logger::Config::default());

    yew::Renderer::<ruwm_web::App>::new().render();
}
