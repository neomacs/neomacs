use std::time::Duration;

use tokio::time;

const REDRAW_FPS: f32 = 60.0;

pub fn redraw() {
    // TODO
}

pub fn start_redraw_loop() {
    let mut interval = time::interval(Duration::from_secs(1).div_f32(REDRAW_FPS));
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            redraw();
        }
    });
}
