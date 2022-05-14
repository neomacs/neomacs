pub mod buffer;
pub mod redraw;
pub mod events;

pub fn start_main_loop() {
    let mut event_loop = events::EventLoop::new();
    event_loop.start_loop();
    redraw::start_redraw_loop();
}
