//! ExecuTorch planned buffers are distinct from Ethos NPU base-pointer regions.
use crate::{
    graph::{ArgumentRole, arguments},
    pte::{Method, Tensor, Value},
    vela::{InspectionSection, MemoryRange},
};
use std::collections::BTreeSet;

pub fn tensor_bytes(t: &Tensor) -> Option<u64> {
    if t.shape_dynamism == 2 || t.storage_offset != 0 {
        return None;
    }
    let bytes: u64 = match t.dtype.as_str() {
        "uint8" | "int8" | "bool" | "qint8" | "quint8" | "float8e5m2" | "float8e4m3fn"
        | "float8e5m2fnuz" | "float8e4m3fnuz" => 1,
        "int16" | "uint16" | "float16" | "bfloat16" | "bits16" => 2,
        "int32" | "uint32" | "float32" | "qint32" => 4,
        "int64" | "uint64" | "float64" => 8,
        _ => return None,
    };
    t.shape
        .iter()
        .try_fold(bytes, |n, d| n.checked_mul(u64::try_from(*d).ok()?))
}
pub fn tensor_range(value: &Value, id: usize) -> Option<MemoryRange> {
    let t = value.tensor.as_ref()?;
    let (arena, offset) = t.allocation?;
    let length = tensor_bytes(t);
    let note = if t.shape_dynamism == 1 {
        "Data footprint calculated from the recorded upper-bound shape. Runtime dimensions may be smaller. Allocator padding and tensor lifetimes are not recorded."
    } else if length.is_none() {
        "Data length unavailable: unbounded shape, unsupported datatype, non-zero storage offset, or size overflow. The marker identifies the recorded allocation offset."
    } else {
        "Data footprint calculated from datatype and shape, excluding allocator padding. Overlapping spans may be aliases or storage reused at different execution times; they do not imply simultaneous allocations."
    };
    Some(MemoryRange {
        arena: Some(arena),
        region: 0,
        internal: false,
        offset,
        length,
        label: format!("v{id}"),
        command: None,
        note: note.into(),
        transfer: vec![
            ("Tensor value".into(), format!("v{id}")),
            ("Datatype".into(), t.dtype.clone()),
            ("Dimensions".into(), format!("{:?}", t.shape)),
            (
                "Shape interpretation".into(),
                if t.shape_dynamism == 1 {
                    "Upper bound"
                } else if t.shape_dynamism == 2 {
                    "Unbounded / runtime"
                } else {
                    "Static"
                }
                .into(),
            ),
        ],
    })
}
pub fn ids(m: &Method) -> BTreeSet<u32> {
    m.memory_sizes
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, n)| **n > 0)
        .map(|(i, _)| i as u32)
        .chain(
            m.values
                .iter()
                .filter_map(|v| v.tensor.as_ref()?.allocation.map(|a| a.0)),
        )
        .collect()
}
pub fn capacity(m: &Method, id: u32) -> Option<u64> {
    if id == 0 {
        return None;
    }
    u64::try_from(*m.memory_sizes.get(id as usize)?).ok()
}
pub fn help() -> InspectionSection {
    InspectionSection::new("Understanding ExecuTorch arenas")
        .field("Arena ID", "An ID for a method's planned mutable-memory buffer. A method may use several buffers. The runtime supplies each buffer's base address; an arena ID is not an NPU region index.")
        .field("Tensor address", "Runtime base of the selected arena + the tensor's byte offset. The PTE records offsets and planned sizes, not physical base addresses.")
        .field("Reserved entry", "Buffer-size entry 0 is reserved for constants and is not counted as a planned activation arena.")
        .field("Reading the map", "A capacity row shows the planned buffer extent; tensor rows show their data spans at offsets in the same buffer. Spans may overlap through aliases or reuse. This view does not infer live allocations or free space.")
}
fn capacity_range(m: &Method, id: u32) -> MemoryRange {
    MemoryRange {
        arena: Some(id), region: 0, internal: false, offset: 0, length: capacity(m, id),
        label: format!("Arena {id} capacity"), command: None,
        note: if id == 0 { "Reserved memory ID 0; no planned arena capacity is inferred." } else { "Planned buffer capacity recorded for this method. The runtime must supply storage for this buffer." }.into(),
        transfer: vec![("Method".into(), m.name.clone())],
    }
}
pub fn ranges(m: &Method) -> Vec<MemoryRange> {
    let mut result = Vec::new();
    for id in ids(m) {
        let capacity = capacity(m, id);
        result.push(capacity_range(m, id));
        let mut tensors: Vec<_> = m
            .values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| tensor_range(v, i))
            .filter(|r| r.arena == Some(id))
            .collect();
        tensors.sort_by_key(|r| r.offset);
        for range in &mut tensors {
            range.transfer.push(("Method".into(), m.name.clone()));
            range.transfer.push((
                "Arena capacity".into(),
                capacity
                    .map(|n| format!("{n} bytes"))
                    .unwrap_or_else(|| "Not recorded".into()),
            ));
            if let (Some(capacity), Some(length)) = (capacity, range.length)
                && range
                    .offset
                    .checked_add(length)
                    .is_none_or(|end| end > capacity)
            {
                range.transfer.push((
                    "Bounds".into(),
                    "Recorded tensor footprint exceeds the planned arena capacity".into(),
                ));
            }
            let value = range.label.trim_start_matches('v').parse::<i32>().unwrap();
            let mut uses = Vec::new();
            for (i, op) in m.instructions.iter().enumerate() {
                let mut roles = BTreeSet::new();
                for arg in arguments(m, i).into_iter().filter(|a| a.value == value) {
                    roles.insert(match arg.role {
                        ArgumentRole::Input => "input",
                        ArgumentRole::Output => "output",
                        ArgumentRole::Unknown => "argument",
                    });
                }
                if !roles.is_empty() {
                    uses.push(format!(
                        "#{i} {} ({})",
                        op.name,
                        roles.into_iter().collect::<Vec<_>>().join(" / ")
                    ));
                }
            }
            range
                .transfer
                .push(("Operator uses (linear order)".into(), uses.join("\n")));
        }
        result.extend(tensors);
    }
    result
}
pub fn summary(m: &Method) -> InspectionSection {
    let count = ids(m).len();
    let mut section = InspectionSection::new("Planned memory arenas")
        .field("Planned arena count", count)
        .field("Meaning", "Each arena is a separate method-local planned memory buffer. IDs identify buffers, not a count. NPU regions are a separate address mapping.");
    for id in ids(m) {
        section = section.memory_link(
            &format!("Arena {id}"),
            capacity(m, id)
                .map(|n| format!("{n} bytes (0x{n:x}) planned capacity"))
                .unwrap_or_else(|| "Capacity not recorded".into()),
            capacity_range(m, id),
        );
    }
    section
}
