#![feature(map_first_last)]
#![allow(warnings)]

mod core;
mod primary;
//mod messages;
mod error;
mod election;
mod general;
mod node;
mod vote;

pub use primary::Primary;
pub use node::NodeId;
pub use crate::core::now;

extern crate time;