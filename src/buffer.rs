//! The core text-editing datastructure

use std::{
    cmp,
    ops::{Range, RangeBounds},
};

use ropey::{Rope, RopeSlice};

#[derive(Copy, Clone)]
struct Point {
    char_idx: usize,
}

impl Point {
    fn new() -> Self {
        Self { char_idx: 0 }
    }

    fn at_idx(char_idx: usize) -> Self {
        Self { char_idx }
    }

    fn set_char_idx(&mut self, char_idx: usize) {
        self.char_idx = char_idx;
    }
}

pub struct Buffer {
    name: String,
    contents: Rope,
    point: Point,
    mark: Option<Point>,
}

/// The buffer is the fundamental text editing datastructure. Buffers
/// have a globally unique name, some text contents, a point, and
/// sometimes a mark. The point is the current location of the user
/// cursor in the buffer. The mark (set with the `set_mark` function)
/// represents a second point in the buffer, allowing users to select
/// text between the two points. The text between the two points is
/// called the region.
///
/// # Examples
/// ```
/// # use neomacs::buffer::Buffer;
/// let mut buf = Buffer::new("hello-world.txt");
///
/// buf.insert("Hello, world!");
/// assert_eq!(buf.contents(), "Hello, world!");
///
/// buf.set_point(7);
/// buf.insert("beautiful ");
/// assert_eq!(buf.contents(), "Hello, beautiful world!");
///
/// buf.set_mark(7);
/// buf.remove();
/// assert_eq!(buf.contents(), "Hello, world!");
///
/// assert_eq!(buf.slice(0..5), "Hello");
/// buf.set_point(0);
/// buf.set_mark(5);
/// assert_eq!(buf.region_slice().unwrap(), "Hello");
/// ```
impl Buffer {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            contents: Rope::new(),
            point: Point::new(),
            mark: None,
        }
    }

    /// The buffer's name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// The buffer's contents as a [`Rope`](https://docs.rs/ropey/1.4.1/ropey/struct.Rope.html#).
    pub fn contents(&self) -> &Rope {
        &self.contents
    }

    /// Sets the point to `char_idx`.
    pub fn set_point(&mut self, char_idx: usize) {
        self.point.set_char_idx(char_idx);
    }

    /// Sets the mark to be the same index as the point.
    pub fn set_mark_to_point(&mut self) {
        self.set_mark(self.point.char_idx);
    }

    /// Sets the mark to `char_idx`.
    pub fn set_mark(&mut self, char_idx: usize) {
        match &mut self.mark {
            Some(mark) => mark.set_char_idx(char_idx),
            None => self.mark = Some(Point::at_idx(char_idx)),
        }
    }

    /// Clears the mark by setting it to `None`.
    pub fn clear_mark(&mut self) {
        self.mark = None;
    }

    /// Inserts `text` at the current index of the point, moving the
    /// point to the end of the newly inserted text.
    pub fn insert(&mut self, text: &str) {
        self.insert_at(self.point.char_idx, text);
        self.point
            .set_char_idx(self.point.char_idx + text.chars().count());
    }

    /// Inserts `text` at the `char_idx`, does not move the point.
    pub fn insert_at(&mut self, char_idx: usize, text: &str) {
        self.contents.insert(char_idx, text);
    }

    /// Returns the range in the buffer delineated by the point and
    /// mark, if the mark is set. Whichever index is lower will be the
    /// bottom of the range. The range is inclusive of the low index
    /// and exclusive of the high index.
    fn region(&self) -> Option<Range<usize>> {
        self.mark.map(|mark| Range {
            start: cmp::min(self.point.char_idx, mark.char_idx),
            end: cmp::max(self.point.char_idx, mark.char_idx),
        })
    }

    /// Deletes the text that is currently in the region, if a region
    /// exists.
    pub fn remove(&mut self) {
        if let Some(region) = self.region() {
            self.remove_at(region)
        }
    }

    /// Removes the text in the bounds delineated by the `char_range`.
    pub fn remove_at<R: RangeBounds<usize>>(&mut self, char_range: R) {
        self.contents.remove(char_range);
    }

    /// Returns a
    /// [`RopeSlice`](https://docs.rs/ropey/1.4.1/ropey/struct.RopeSlice.html)
    /// of the part of the buffer currently in the region, if one
    /// exists.
    pub fn region_slice(&self) -> Option<RopeSlice<'_>> {
        self.region().map(|region| self.slice(region))
    }

    /// Returns a
    /// [`RopeSlice`](https://docs.rs/ropey/1.4.1/ropey/struct.RopeSlice.html)
    /// of the part of the buffer delineated by the `char_range`.
    pub fn slice<R: RangeBounds<usize>>(&self, char_range: R) -> RopeSlice<'_> {
        self.contents.slice(char_range)
    }
}
