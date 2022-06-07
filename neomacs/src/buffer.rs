//! The core text-editing datastructure
use crate::{
    error::{NeomacsError, Result},
    point::Point,
};
use anyhow::anyhow;
use maplit::hashmap;

use std::{
    cmp,
    collections::HashMap,
    io::Cursor,
    ops::{Range, RangeBounds},
    path::{Path, PathBuf},
    time::SystemTime,
};

use ropey::{Rope, RopeSlice};
use tokio::{fs::File, io::AsyncReadExt};

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
#[derive(Debug)]
pub struct Buffer {
    name: String,
    contents: Rope,
    visited_file: Option<PathBuf>,
    point: Point,
    mark: Option<Point>,
    modified: bool,
    modified_time: SystemTime,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new("(untitled)")
    }
}

impl Buffer {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            contents: Rope::new(),
            visited_file: None,
            point: Point::new(),
            mark: None,
            modified: false,
            modified_time: SystemTime::now(),
        }
    }

    /// Opens a buffer visiting the file at `path`.  If the file
    /// already exists, sets the buffer's contents and modified time
    /// from the file's.
    pub async fn visit_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let modified_time: SystemTime;
        let contents: Rope;
        if let Ok(mut file) = File::open(path.as_ref()).await {
            modified_time = file.metadata().await?.modified()?;
            contents = {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).await?;
                Rope::from_reader(Cursor::new(buf))?
            }
        } else {
            modified_time = SystemTime::now();
            contents = Rope::new();
        }
        let name = path
            .as_ref()
            .file_name()
            .ok_or_else(|| NeomacsError::from(anyhow!("Invalid file path")))?
            .to_str()
            .ok_or_else(|| NeomacsError::from(anyhow!("File path is not valid unicode")))?
            .to_string();
        Ok(Self {
            name,
            contents,
            visited_file: Some(path.as_ref().to_owned()),
            point: Point::new(),
            mark: None,
            modified: false,
            modified_time,
        })
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

/// Container to keep track of currently open buffers
#[derive(Debug)]
pub struct BufferState {
    buffers: HashMap<String, Buffer>,
    current_buffer: String,
}

impl Default for BufferState {
    fn default() -> Self {
        let empty_buf = Buffer::default();
        let empty_buf_name = empty_buf.name().to_string();
        Self {
            buffers: hashmap! {empty_buf_name.clone() => empty_buf},
            current_buffer: empty_buf_name,
        }
    }
}

impl BufferState {
    pub fn get_buffer<S: Into<String>>(&self, name: S) -> Option<&Buffer> {
        self.buffers.get(&name.into())
    }

    pub fn get_buffer_mut<S: Into<String>>(&mut self, name: S) -> Option<&mut Buffer> {
        self.buffers.get_mut(&name.into())
    }

    pub fn current_buffer(&self) -> &Buffer {
        self.buffers.get(&self.current_buffer).unwrap()
    }

    pub fn current_buffer_mut(&mut self) -> &mut Buffer {
        self.buffers.get_mut(&self.current_buffer).unwrap()
    }

    pub fn set_current_buffer<S: Into<String>>(&mut self, name: S) -> Result<()> {
        let name = name.into();
        if !self.buffers.contains_key(&name) {
            return Err(NeomacsError::DoesNotExist(format!("Buffer {}", name)));
        }
        self.current_buffer = name;
        Ok(())
    }

    pub fn add_buffer(&mut self, buffer: Buffer) -> Result<()> {
        let name = buffer.name().to_string();
        if self.buffers.contains_key(&name) {
            return Err(NeomacsError::AlreadyExists(format!("Buffer {}", name)));
        }
        self.buffers.insert(name, buffer);
        Ok(())
    }

    pub fn delete_buffer<S: Into<String>>(&mut self, name: S) -> Result<()> {
        let name = name.into();
        if self.current_buffer().name() == name {
            // TODO put together a smarter solution for choosing the buffer to switch to
            // e.g. switch to most recently active
            let next_buf = self
                .buffers
                .iter()
                .find(|(n, _)| n.as_str() != name.as_str())
                .map(|(n, _)| n)
                .cloned();
            if let Some(next) = next_buf {
                self.set_current_buffer(next)?;
            } else {
                let empty_buf = Buffer::default();
                let empty_name = empty_buf.name().to_string();
                self.add_buffer(empty_buf)?;
                self.set_current_buffer(empty_name)?;
            }
        }
        self.buffers.remove(&name);
        Ok(())
    }

    pub fn rename_buffer<S1: Into<String>, S2: Into<String>>(
        &mut self,
        old_name: S1,
        new_name: S2,
    ) -> Result<()> {
        let old_name = old_name.into();
        let new_name = new_name.into();
        if self.buffers.contains_key(&new_name) {
            return Err(NeomacsError::AlreadyExists(format!("Buffer {}", new_name)));
        }
        if let Some(mut buf) = self.buffers.remove(&old_name) {
            buf.name = new_name;
            let buf_name = buf.name().to_string();
            self.buffers.insert(buf_name.clone(), buf);
            if self.current_buffer == old_name {
                self.current_buffer = buf_name;
            }
        }
        Ok(())
    }
}
