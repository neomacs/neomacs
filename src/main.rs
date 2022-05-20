use neomacs::app::App;

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut app = App::new();
    app.start().await;
}
