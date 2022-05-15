use neomacs::App;

#[tokio::main]
async fn main() {
    let app = App::new();
    app.start_main_loop();
}
