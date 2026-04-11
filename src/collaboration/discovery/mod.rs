//! NAT 类型检测模块

#![allow(unused_imports)]

/// NAT 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NatType {
    /// 未知
    Unknown,
    /// 对称型 NAT（需要打洞）
    Symmetric,
    /// 圆锥型 NAT（可穿透）
    Cone,
}

mod stun;
pub use stun::{get_public_ip_port, is_nat_symmetric};
