use neomacs::App;

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.start().await;
}
