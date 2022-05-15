use neomacs::App;

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.start_main_loop();
}
