#[derive(Copy, Clone, Debug)]
pub struct Point {
    pub char_idx: usize,
}

impl Point {
    pub fn new() -> Self {
        Self { char_idx: 0 }
    }

    pub fn at_idx(char_idx: usize) -> Self {
        Self { char_idx }
    }

    pub fn set_char_idx(&mut self, char_idx: usize) {
        self.char_idx = char_idx;
    }
}

impl Default for Point {
    fn default() -> Self {
        Self::new()
    }
}
