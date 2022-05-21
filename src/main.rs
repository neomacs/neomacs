use neomacs::app::App;

#[tokio::main]
async fn main() {
    env_logger::init();
    if cfg!(unix) {
        let mut app = App::init_unix().await.expect("Failed to initialize app");
        app.start().await.expect("Application failure!")
    } else {
        let mut app = App::init_tcp().await.expect("Failed to initialize app");
        app.start().await.expect("Application failure!")
    }
}
