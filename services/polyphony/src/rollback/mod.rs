use pallas::network::miniprotocols::Point;
use serde::{Deserialize, Serialize};

use crate::model::StorageAction;

pub mod buffer;

#[derive(Serialize, Deserialize, Clone)]
pub enum SerializablePoint {
    Origin,
    Specific(u64, Vec<u8>),
}

impl From<Point> for SerializablePoint {
    fn from(point: Point) -> Self {
        match point {
            Point::Origin => SerializablePoint::Origin,
            Point::Specific(s, h) => SerializablePoint::Specific(s, h),
        }
    }
}

impl From<SerializablePoint> for Point {
    fn from(val: SerializablePoint) -> Self {
        match val {
            SerializablePoint::Origin => Point::Origin,
            SerializablePoint::Specific(s, h) => Point::Specific(s, h),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PersistentBufferValue {
    pub point: Point,
    pub inverse_actions: Vec<StorageAction>,
}
