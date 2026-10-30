//! Graph layout and camera math are independent of the GPU backend.
use crate::{pte::Method, schemas};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Selection {
    Operator(usize),
    Value(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentRole {
    Input,
    Output,
    Unknown,
}
pub fn argument_roles(m: &Method, index: usize) -> Vec<ArgumentRole> {
    let op = &m.instructions[index];
    if op.kind == 1
        && let Some(schema) = schemas::lookup(&op.name, op.args.len())
    {
        return schema
            .arguments
            .iter()
            .map(|a| {
                if a.output {
                    ArgumentRole::Output
                } else {
                    ArgumentRole::Input
                }
            })
            .chain(schema.returns.iter().map(|_| ArgumentRole::Output))
            .collect();
    }
    if let Some(vela) = op
        .delegate
        .and_then(|d| m.delegates.get(d))
        .and_then(|d| d.vela.as_ref())
    {
        let count = vela.inputs.len() + vela.outputs.len();
        // EthosUBackend's ABI is inputs followed by outputs, as described by
        // VelaIOs: executorch/backends/arm/runtime/EthosUBackend.cpp (execute).
        if count > 0 && count <= op.args.len() {
            return (0..op.args.len())
                .map(|i| {
                    if i < vela.inputs.len() {
                        ArgumentRole::Input
                    } else if i < count {
                        ArgumentRole::Output
                    } else {
                        ArgumentRole::Unknown
                    }
                })
                .collect();
        }
    }
    op.args
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if op.kind == 3 {
                return if i == 0 {
                    ArgumentRole::Input
                } else {
                    ArgumentRole::Output
                };
            }
            if matches!(op.kind, 4 | 5) {
                return ArgumentRole::Input;
            }
            if m.instructions.len() == 1 {
                match (m.inputs.contains(v), m.outputs.contains(v)) {
                    (true, false) => return ArgumentRole::Input,
                    (false, true) => return ArgumentRole::Output,
                    _ => {}
                }
            }
            ArgumentRole::Unknown
        })
        .collect()
}
#[derive(Clone, Debug)]
pub struct Argument {
    pub value: i32,
    pub name: String,
    pub role: ArgumentRole,
    schema_parameter: bool,
}
pub fn arguments(m: &Method, index: usize) -> Vec<Argument> {
    let op = &m.instructions[index];
    let schema = (op.kind == 1)
        .then(|| schemas::lookup(&op.name, op.args.len()))
        .flatten();
    op.args
        .iter()
        .zip(argument_roles(m, index))
        .enumerate()
        .map(|(i, (&value, role))| {
            let formal = schema.and_then(|s| {
                s.arguments
                    .get(i)
                    .or_else(|| s.returns.get(i - s.arguments.len()))
            });
            let name = formal
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("argument {i}"));
            let schema_parameter = formal.is_some_and(|p| {
                !p.datatype.starts_with("Tensor") || m.values[value as usize].kind == "None"
            });
            Argument {
                value,
                name,
                role,
                schema_parameter,
            }
        })
        .collect()
}
pub fn constant_tensor(m: &Method, id: i32) -> bool {
    m.values
        .get(id as usize)
        .and_then(|v| v.tensor.as_ref())
        .is_some_and(|t| t.data_buffer != 0 && t.allocation.is_none())
}
pub fn parameter(m: &Method, arg: &Argument) -> bool {
    let value = &m.values[arg.value as usize];
    arg.schema_parameter
        && !constant_tensor(m, arg.value)
        && value.tensor.is_none()
        && value.references.is_empty()
        && arg.role != ArgumentRole::Output
        && !m.inputs.contains(&arg.value)
        && !m.outputs.contains(&arg.value)
        && !m.instructions.iter().enumerate().any(|(i, op)| {
            op.args
                .iter()
                .zip(argument_roles(m, i))
                .any(|(&id, role)| id == arg.value && role == ArgumentRole::Output)
        })
}
#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub selection: Selection,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub title: String,
    pub subtitle: String,
    pub properties: Vec<(String, String)>,
    pub delegate: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub execution: bool,
    pub directed: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub width: f32,
    pub height: f32,
}
#[path = "graph/layout.rs"]
mod layout;
impl Graph {
    pub fn method(m: &Method) -> Self {
        layout::method(m)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.,
            x: 16.,
            y: 16.,
        }
    }
}
impl Camera {
    pub fn reveal(&mut self, node: &Node, width: f32, height: f32) {
        let padding = 24_f32.min(width.min(height) / 4.).max(0.);
        let zoom = self
            .zoom
            .min((width - 2. * padding) / node.w)
            .min((height - 2. * padding) / node.h)
            .clamp(0.0001, 4.);
        if zoom < self.zoom {
            self.zoom_at(zoom, (width / 2., height / 2.));
        }
        let left = self.x + node.x * self.zoom;
        let right = left + node.w * self.zoom;
        let top = self.y + node.y * self.zoom;
        let bottom = top + node.h * self.zoom;
        self.x += (padding - left).max(0.) - (right - width + padding).max(0.);
        self.y += (padding - top).max(0.) - (bottom - height + padding).max(0.);
    }
    pub fn zoom_at(&mut self, new: f32, anchor: (f32, f32)) {
        let new = new.clamp(0.0001, 4.);
        let ratio = new / self.zoom;
        self.x = anchor.0 - (anchor.0 - self.x) * ratio;
        self.y = anchor.1 - (anchor.1 - self.y) * ratio;
        self.zoom = new;
    }
    pub fn fit(&mut self, graph: &Graph, width: f32, height: f32) {
        self.zoom = ((width - 40.) / graph.width)
            .min((height - 40.) / graph.height)
            .clamp(0.0001, 2.);
        self.x = (width - graph.width * self.zoom) / 2.;
        self.y = (height - graph.height * self.zoom) / 2.;
    }
}
