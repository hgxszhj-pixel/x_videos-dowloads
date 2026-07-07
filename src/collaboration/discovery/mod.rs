//! NAT 类型检测模块

#![allow(unused_imports)]

mod stun;
pub use stun::{get_public_ip_port, is_nat_symmetric};
