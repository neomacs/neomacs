use crate::{
    buffer::Buffer,
    error::{NeomacsError, Result},
    point::Point,
};

/// A splittable view onto a buffer ("window" in Emacs terms)
pub enum Pane {
    /// An "internal" pane split into two panes
    Split {
        direction: SplitDirection,
        split_location: SplitLocation,
        first_child: Box<Pane>,
        second_child: Box<Pane>,
    },
    /// A "live" pane viewing a buffer
    Live { buffer: Buffer, point: Point },
}

pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Specifies the location of a split in a split pane as a percentage
/// of the total pane width/height. For example, a window split 60/40
/// horizontally would have a value of 0.6, meaning the split occurs
/// at (pane width * 0.6)
pub struct SplitLocation(f64);

impl SplitLocation {
    pub fn new(split: f64) -> Result<Self> {
        if (0.0..1.0).contains(&split) {
            Err(NeomacsError::InvalidSplit(split))
        } else {
            Ok(Self(split))
        }
    }
}

impl From<SplitLocation> for f64 {
    fn from(split: SplitLocation) -> Self {
        split.0
    }
}

impl TryFrom<f64> for SplitLocation {
    type Error = NeomacsError;

    fn try_from(value: f64) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}
