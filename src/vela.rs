//! Vela container and COP1 driver payload, decoded without executing model data.
use crate::flat::Reader;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[path = "vela/descriptions.rs"]
mod descriptions;
#[path = "vela/memory.rs"]
mod memory;
#[path = "generated/opcodes.rs"]
mod opcodes;
pub use memory::{addressing_help, region_role};

#[derive(Debug, Serialize)]
pub struct Vela {
    pub blocks: Vec<Block>,
    pub inputs: Vec<Io>,
    pub outputs: Vec<Io>,
    pub scratch_size: Option<u32>,
    pub config: Option<Hardware>,
    pub instructions: Vec<Command>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Block {
    pub name: String,
    pub size: usize,
    pub external_key: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Io {
    pub shape: Vec<i32>,
    pub element_bytes: i32,
    pub offset: i32,
    pub region: i32,
}
#[derive(Debug, Serialize)]
pub struct Hardware {
    pub architecture: String,
    pub config_word: u32,
    pub id_word: u32,
    pub macs_per_cycle: u32,
    pub stream_version: u32,
}
#[derive(Debug, Serialize)]
pub struct Command {
    pub offset: usize,
    pub raw: u32,
    pub payload: Option<u32>,
    pub name: String,
    pub value: u64,
    pub fields: BTreeMap<String, u64>,
}

#[derive(Clone, Debug)]
pub struct InspectionSection {
    pub title: String,
    pub fields: Vec<(String, String)>,
    pub collapsed: bool,
    pub memory_links: BTreeMap<usize, MemoryRange>,
}
impl InspectionSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fields: vec![],
            collapsed: false,
            memory_links: BTreeMap::new(),
        }
    }
    pub fn field(mut self, label: impl Into<String>, value: impl ToString) -> Self {
        self.fields.push((label.into(), value.to_string()));
        self
    }
    pub fn collapsed(mut self) -> Self {
        self.collapsed = true;
        self
    }
    pub fn memory_link(mut self, label: &str, value: String, target: MemoryRange) -> Self {
        self.memory_links.insert(self.fields.len(), target);
        self.fields.push((label.into(), value));
        self
    }
}
#[derive(Clone, Debug)]
pub struct MemoryRange {
    pub arena: Option<u32>,
    pub region: u64,
    pub internal: bool,
    pub offset: u64,
    pub length: Option<u64>,
    pub label: String,
    pub command: Option<usize>,
    pub note: String,
    pub transfer: Vec<(String, String)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemorySpace {
    Region(u64),
    Internal,
    Arena(u32),
}
impl MemoryRange {
    pub fn space(&self) -> MemorySpace {
        if let Some(id) = self.arena {
            MemorySpace::Arena(id)
        } else if self.internal {
            MemorySpace::Internal
        } else {
            MemorySpace::Region(self.region)
        }
    }
}
type RegisterState<'a> = BTreeMap<&'a str, u64>;
type ChainShapes = BTreeMap<u64, [Option<u64>; 3]>;

fn parse_io(data: &[u8]) -> Result<Vec<Io>> {
    let r = Reader(data);
    let n = r.u32(0)? as usize;
    if n == 0 {
        return Ok(vec![]);
    }
    ensure!(n <= 100_000, "too many Vela I/O descriptors");
    let width = (data.len() - 4) / n;
    ensure!(
        (width == 28 || width == 36) && 4 + n * width == data.len(),
        "unsupported Vela I/O descriptor size {width}"
    );
    let rank = width / 4 - 3;
    (0..n)
        .map(|i| {
            let p = 4 + i * width;
            Ok(Io {
                shape: (0..rank)
                    .map(|j| Ok(r.u32(p + j * 4)? as i32))
                    .collect::<Result<_>>()?,
                element_bytes: r.u32(p + rank * 4)? as i32,
                offset: r.u32(p + (rank + 1) * 4)? as i32,
                region: r.u32(p + (rank + 2) * 4)? as i32,
            })
        })
        .collect()
}
impl Vela {
    pub fn parse<'a>(data: &'a [u8], external: impl Fn(&str) -> Result<&'a [u8]>) -> Result<Self> {
        let r = Reader(data);
        let mut p = 0;
        let mut v = Self {
            blocks: vec![],
            inputs: vec![],
            outputs: vec![],
            scratch_size: None,
            config: None,
            instructions: vec![],
            warnings: vec![],
        };
        ensure!(
            data.starts_with(b"vela_bin_stream\0"),
            "not a Vela block stream"
        );
        let mut footer = false;
        while p < data.len() {
            r.bytes(p, 32)?;
            let name_bytes = r.bytes(p, 16)?;
            let name =
                std::str::from_utf8(name_bytes.split(|b| *b == 0).next().unwrap_or_default())?
                    .to_owned();
            let size = r.u32(p + 16)? as usize;
            let flag = r.u8(p + 20)?;
            ensure!(flag <= 1, "unknown external block flag {flag}");
            let payload = r.bytes(p + 32, size)?;
            let external_key = if flag == 1 {
                Some(
                    std::str::from_utf8(payload)?
                        .trim_end_matches('\0')
                        .to_owned(),
                )
            } else {
                None
            };
            let payload = if let Some(key) = &external_key {
                external(key).with_context(|| format!("external block {name}: {key}"))?
            } else {
                payload
            };
            v.blocks.push(Block {
                name: name.clone(),
                size: payload.len(),
                external_key,
            });
            match name.as_str() {
                "cmd_data" => v.driver(payload)?,
                "inputs" => v.inputs = parse_io(payload)?,
                "outputs" => v.outputs = parse_io(payload)?,
                "scratch_size" => v.scratch_size = Some(Reader(payload).u32(0)?),
                "vela_end_stream" => {
                    footer = true;
                    break;
                }
                _ => {}
            }
            p = p
                .checked_add(32)
                .and_then(|p| p.checked_add((size + 15) & !15))
                .context("block offset overflow")?;
        }
        ensure!(footer, "missing vela_end_stream block");
        if v.instructions.is_empty() {
            v.warnings.push("No NPU command stream found".into());
        }
        Ok(v)
    }
    fn driver(&mut self, data: &[u8]) -> Result<()> {
        ensure!(
            data.starts_with(b"COP1"),
            "unsupported driver payload (expected COP1)"
        );
        let r = Reader(data);
        let mut p = 4;
        while p < data.len() {
            let tag = r.u32(p)?;
            p += 4;
            match tag & 255 {
                1 => {
                    let cfg = r.u32(p)?;
                    let id = r.u32(p + 4)?;
                    p += 8;
                    let product = cfg >> 28;
                    let architecture = match product {
                        0 => "Ethos-U55",
                        1 => "Ethos-U65",
                        2 => "Ethos-U85",
                        _ => bail!("unsupported Ethos product {product}"),
                    };
                    self.config = Some(Hardware {
                        architecture: architecture.into(),
                        config_word: cfg,
                        id_word: id,
                        macs_per_cycle: 1 << (cfg & 15),
                        stream_version: (cfg >> 4) & 15,
                    });
                }
                2 => {
                    let n = ((tag >> 16) | ((tag & 0xff00) << 8)) as usize;
                    let bytes = n.checked_mul(4).context("command length overflow")?;
                    r.bytes(p, bytes)?;
                    self.decode(r.bytes(p, bytes)?, p)?;
                    p += bytes;
                }
                5 => {}
                3 | 4 => self
                    .warnings
                    .push(format!("Driver action 0x{tag:08x} at 0x{:x}", p - 4)),
                _ => bail!("unknown driver action 0x{tag:08x}"),
            }
        }
        Ok(())
    }
    fn decode(&mut self, data: &[u8], base: usize) -> Result<()> {
        let cfg = self
            .config
            .as_ref()
            .context("command stream precedes hardware configuration")?;
        let u85 = cfg.architecture == "Ethos-U85";
        if cfg.stream_version > 1 {
            self.warnings.push(format!(
                "Unrecognized stream version {}; unknown instructions are retained",
                cfg.stream_version
            ));
        }
        let r = Reader(data);
        let mut p = 0;
        while p < data.len() {
            let start = p;
            let raw = r.u32(p)?;
            p += 4;
            let control = (raw >> 14) & 3;
            ensure!(
                control <= 1,
                "unsupported command control {control} at 0x{:x}",
                base + start
            );
            let payload = if control == 1 {
                let value = r.u32(p)?;
                p += 4;
                Some(value)
            } else {
                None
            };
            let code = (raw & 0x3ff) as u16;
            let name = opcodes::name(u85, payload.is_some(), code)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("UNKNOWN_0x{:04x}", raw & 65535));
            let full = raw as u64 | ((payload.unwrap_or(0) as u64) << 32);
            let fields = opcodes::fields(u85, &name)
                .iter()
                .map(|&(n, s, w)| (n.into(), (full >> s) & ((1u64 << w) - 1)))
                .collect();
            let value = if let Some(payload) = payload {
                payload as u64 | ((raw as u64 >> 16) << 32)
            } else {
                (raw >> 16) as u64
            };
            if name.starts_with("UNKNOWN") {
                self.warnings.push(format!(
                    "Unknown opcode at 0x{:x}: 0x{raw:08x}",
                    base + start
                ));
            }
            self.instructions.push(Command {
                offset: base + start,
                raw,
                payload,
                name,
                value,
                fields,
            });
        }
        Ok(())
    }
    pub fn region_uses(&self) -> BTreeMap<u64, BTreeSet<String>> {
        let mut usage: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
        let u85 = self
            .config
            .as_ref()
            .is_some_and(|h| h.architecture == "Ethos-U85");
        let mut storage = BTreeMap::new();
        for c in &self.instructions {
            if c.name.ends_with("_PRECISION") {
                storage.insert(c.name.trim_end_matches("_PRECISION"), (c.value >> 14) & 3);
            }
            if c.name.ends_with("_REGION") {
                let internal =
                    c.fields.get("region_mode") == Some(&1) || c.fields.get("internal") == Some(&1);
                let chained = u85 && storage.get(c.name.trim_end_matches("_REGION")) == Some(&2);
                if !internal && !chained {
                    usage.entry(c.value & 7).or_default().insert(
                        c.name
                            .trim_start_matches("NPU_SET_")
                            .trim_end_matches("_REGION")
                            .into(),
                    );
                }
            }
        }
        for (label, ios) in [("Input", &self.inputs), ("Output", &self.outputs)] {
            for io in ios.iter().filter(|io| io.region >= 0) {
                usage
                    .entry(io.region as u64)
                    .or_default()
                    .insert(label.into());
            }
        }
        usage
    }
    pub fn regions(&self) -> String {
        let usage = self.region_uses();
        let mut text = String::from(
            "NPU addresses are BASEP[region] + byte offset. Physical base addresses and AXI/SRAM/DRAM placement are selected by the runtime and are not encoded here. Region numbers alone do not identify a physical memory.\n\n",
        );
        for (n, roles) in usage {
            let conventional = format!("Usual Vela role: {}", region_role(n));
            text += &format!(
                "Region {n} — {conventional}\nObserved uses: {}\n",
                roles.into_iter().collect::<Vec<_>>().join(", ")
            );
            for (kind, ios) in [("input", &self.inputs), ("output", &self.outputs)] {
                for (i, io) in ios
                    .iter()
                    .enumerate()
                    .filter(|(_, io)| io.region as u64 == n)
                {
                    text += &format!(
                        "  {kind} {i}: {:?}, {} B/element, offset 0x{:x}\n",
                        io.shape, io.element_bytes, io.offset
                    );
                }
            }
            text.push('\n');
        }
        text
    }
    fn state_at(&self, index: usize) -> (RegisterState<'_>, ChainShapes) {
        let mut state = BTreeMap::new();
        let mut chain_shapes: BTreeMap<u64, [Option<u64>; 3]> = BTreeMap::new();
        for (position, command) in self.instructions.iter().take(index + 1).enumerate() {
            if command.name.starts_with("NPU_SET_") {
                state.insert(command.name.as_str(), command.value);
            }
            if position < index
                && matches!(
                    command.name.as_str(),
                    "NPU_OP_CONV"
                        | "NPU_OP_DEPTHWISE"
                        | "NPU_OP_POOL"
                        | "NPU_OP_ELEMENTWISE"
                        | "NPU_OP_RESIZE"
                )
                && state
                    .get("NPU_SET_OFM_PRECISION")
                    .is_some_and(|p| (p >> 14) & 3 == 2)
                && let Some(&buffer) = state.get("NPU_SET_OFM_REGION")
            {
                chain_shapes.insert(
                    buffer,
                    [
                        "NPU_SET_OFM_HEIGHT_M1",
                        "NPU_SET_OFM_WIDTH_M1",
                        "NPU_SET_OFM_DEPTH_M1",
                    ]
                    .map(|r| state.get(r).map(|n| n + 1)),
                );
            }
        }
        (state, chain_shapes)
    }
    pub fn details(&self, index: usize) -> String {
        let Some(c) = self.instructions.get(index) else {
            return String::new();
        };
        let (state, chain_shapes) = self.state_at(index);
        let get = |suffix: &str| state.get(format!("NPU_SET_{suffix}").as_str()).copied();
        let val = |suffix: &str| {
            get(suffix)
                .map(|n| format!("{n} (0x{n:x})"))
                .unwrap_or_else(|| "not yet configured".into())
        };
        let dim = |suffix: &str| {
            get(suffix)
                .map(|n| (n + 1).to_string())
                .unwrap_or_else(|| "?".into())
        };
        let u85 = self
            .config
            .as_ref()
            .is_some_and(|h| h.architecture == "Ethos-U85");
        let mut text = format!(
            "{}\ncmd_data byte offset 0x{:x}\nWord 0x{:08x}\nParameter {}\n",
            c.name,
            c.offset,
            c.raw,
            c.raw >> 16
        );
        if let Some(p) = c.payload {
            text += &format!("Payload 0x{p:08x}\n");
        }
        text.push('\n');
        text += &descriptions::explain(&c.name, u85);
        if c.name.ends_with("_M1") {
            text += &format!("\nDecoded dimension: {} (stored value + 1)", c.value + 1);
        }
        text += "\n\nDecoded fields\n";
        for (name, value) in &c.fields {
            let kind = match name.as_str() {
                "mode" if c.name == "NPU_OP_POOL" => "pooling_mode",
                "mode" if c.name == "NPU_OP_ELEMENTWISE" => "elementwise_mode",
                "region_mode" => "dma_region_mode",
                "stride_mode" => "dma_stride_mode",
                "idx_mode" => "dma_idx_mode",
                "precision" if c.name.contains("OFM") => "ofm_precision",
                "precision" => "ifm_precision",
                "format" => "data_format",
                other => other,
            };
            let symbol = opcodes::symbol(u85, kind, *value)
                .map(|s| format!(" = {s}"))
                .unwrap_or_default();
            text += &format!("{name}: {value} (0x{value:x}){symbol}\n");
        }
        let address = |prefix: &str, base: &str| -> String {
            let region = get(&format!("{prefix}_REGION"));
            if u85 && get(&format!("{prefix}_PRECISION")).is_some_and(|p| (p >> 14) & 3 == 2) {
                return format!(
                    "internal chaining buffer {} (no external base address)",
                    region.map(|r| r.to_string()).unwrap_or_else(|| "?".into())
                );
            }
            let internal = prefix.starts_with("DMA") && region.is_some_and(|n| n & 0x100 != 0);
            let label = if internal {
                "internal NPU memory".into()
            } else {
                region
                    .map(|r| format!("BASEP[{}]", r & 7))
                    .unwrap_or_else(|| "BASEP[?]".into())
            };
            format!("{label} + {}", val(base))
        };
        if c.name.contains("DMA") {
            text += &format!(
                "\nDMA state at this instruction\nSource: {}\nDestination: {}\nLength per transfer: {} bytes\n",
                address("DMA0_SRC", "DMA0_SRC"),
                address("DMA0_DST", "DMA0_DST"),
                val("DMA0_LEN")
            );
            for suffix in [
                "DMA0_SIZE0",
                "DMA0_SIZE1",
                "DMA0_SRC_STRIDE0",
                "DMA0_SRC_STRIDE1",
                "DMA0_DST_STRIDE0",
                "DMA0_DST_STRIDE1",
                "DMA0_IDX_REGION",
                "DMA0_IDX",
                "DMA0_IDX_MAX",
            ] {
                if get(suffix).is_some() {
                    text += &format!("{suffix}: {}\n", val(suffix));
                }
            }
            text += "DMA may transfer compressed weights or arbitrary bytes; tensor dimensions are not generally encoded for DMA. Strided/indexed mode fields describe repeated transfers. Configuration writes stage a transfer; NPU_OP_DMA_START launches it.\n";
        }
        text += "\nTensor state at this instruction\nShapes below describe the configured NPU stripe/tiles, not necessarily full model tensors. '?' means not yet configured.\n";
        for prefix in ["IFM", "IFM2", "OFM"] {
            let chained =
                u85 && get(&format!("{prefix}_PRECISION")).is_some_and(|p| (p >> 14) & 3 == 2);
            if chained {
                let dims = if prefix == "OFM" {
                    ["OFM_HEIGHT_M1", "OFM_WIDTH_M1", "OFM_DEPTH_M1"].map(|r| get(r).map(|n| n + 1))
                } else {
                    get(&format!("{prefix}_REGION"))
                        .and_then(|buffer| chain_shapes.get(&buffer).copied())
                        .unwrap_or([None; 3])
                };
                let [h, w, d] =
                    dims.map(|n| n.map(|n| n.to_string()).unwrap_or_else(|| "?".into()));
                text += &format!(
                    "\n{prefix}: {}\nH={h}, W={w}, C={d}; {}\n{}\n",
                    address(prefix, &format!("{prefix}_BASE0")),
                    get(&format!("{prefix}_PRECISION"))
                        .map(|p| precision(u85, prefix == "OFM", p))
                        .unwrap_or_else(|| "?".into()),
                    if prefix == "OFM" {
                        "Dimensions from configured OFM shape."
                    } else {
                        "Dimensions from the last preceding chain producer in linear stream order."
                    }
                );
                continue;
            }
            let height = if prefix == "OFM" {
                dim("OFM_HEIGHT_M1")
            } else {
                dim(&format!("{prefix}_HEIGHT0_M1"))
            };
            let width = if prefix == "OFM" {
                dim("OFM_WIDTH_M1")
            } else {
                dim(&format!("{prefix}_WIDTH0_M1"))
            };
            let depth = if prefix == "IFM2" {
                match get("IFM2_BROADCAST") {
                    Some(p) if p & 4 != 0 => "1 (channel broadcast)".into(),
                    Some(_) => format!("{} (shared IFM depth)", dim("IFM_DEPTH_M1")),
                    None => "? (broadcast not configured)".into(),
                }
            } else {
                dim(&format!("{prefix}_DEPTH_M1"))
            };
            let precision = get(&format!("{prefix}_PRECISION"))
                .map(|p| precision(u85, prefix == "OFM", p))
                .unwrap_or_else(|| "?".into());
            text += &format!(
                "\n{prefix} {}: H={height}, W={width}, C={depth}; {precision}\n",
                if prefix == "OFM" {
                    "output"
                } else if prefix == "IFM2" {
                    "second input (binary elementwise only)"
                } else {
                    "input"
                }
            );
            for i in 0..4 {
                let base = format!("{prefix}_BASE{i}");
                if get(&base).is_some() {
                    text += &format!("Tile {i}: {}\n", address(prefix, &base));
                }
            }
            for suffix in ["HEIGHT0_M1", "HEIGHT1_M1", "WIDTH0_M1"] {
                let register = format!("{prefix}_{suffix}");
                if get(&register).is_some() {
                    text += &format!("{suffix}: {}\n", dim(&register));
                }
            }
            for axis in ["X", "Y", "C"] {
                text += &format!(
                    "Stride {axis}: {} bytes\n",
                    val(&format!("{prefix}_STRIDE_{axis}"))
                );
            }
        }
        for prefix in ["WEIGHT", "WEIGHT1", "WEIGHT2", "WEIGHT3", "SCALE", "SCALE1"] {
            if get(&format!("{prefix}_BASE")).is_some() {
                text += &format!(
                    "\n{prefix}: {}, length {} bytes\n",
                    address(
                        if prefix.starts_with("WEIGHT") {
                            "WEIGHT"
                        } else {
                            "SCALE"
                        },
                        &format!("{prefix}_BASE")
                    ),
                    val(&format!("{prefix}_LENGTH"))
                );
            }
        }
        text
    }

    pub fn instruction_sections(&self, index: usize) -> Vec<InspectionSection> {
        let Some(c) = self.instructions.get(index) else {
            return vec![];
        };
        let (state, chains) = self.state_at(index);
        let get = |name: &str| state.get(format!("NPU_SET_{name}").as_str()).copied();
        let value = |name: &str| {
            get(name)
                .map(|v| format!("{v} (0x{v:x})"))
                .unwrap_or_else(|| "Not configured".into())
        };
        let u85 = self
            .config
            .as_ref()
            .is_some_and(|h| h.architecture == "Ethos-U85");
        let address = |prefix: &str, register: &str| {
            let region = get(&format!("{prefix}_REGION"));
            if u85 && get(&format!("{prefix}_PRECISION")).is_some_and(|p| p >> 14 & 3 == 2) {
                return format!(
                    "Internal chain buffer {}",
                    region.map(|n| n.to_string()).unwrap_or_else(|| "?".into())
                );
            }
            let base = match region {
                Some(n) if prefix.starts_with("DMA") && n & 0x100 != 0 => {
                    "Internal NPU memory".into()
                }
                Some(n) => format!("BASEP[{}]", n & 7),
                None => "BASEP[?]".into(),
            };
            format!("{base} + {}", value(register))
        };
        let mut sections = vec![
            InspectionSection::new("Instruction")
                .field("Opcode", &c.name)
                .field("Stream offset", format!("0x{:x}", c.offset))
                .field("Raw word", format!("0x{:08x}", c.raw))
                .field("Value", c.value)
                .collapsed(),
        ];
        if let Some(p) = c.payload {
            sections[0]
                .fields
                .push(("Payload".into(), format!("0x{p:08x}")));
        }
        if c.name.ends_with("_M1") {
            sections[0]
                .fields
                .push(("Decoded dimension".into(), (c.value + 1).to_string()));
        }
        let mut decoded = InspectionSection::new("Decoded fields").collapsed();
        for (name, v) in &c.fields {
            let kind = match name.as_str() {
                "mode" if c.name == "NPU_OP_POOL" => "pooling_mode",
                "mode" if c.name == "NPU_OP_ELEMENTWISE" => "elementwise_mode",
                "region_mode" => "dma_region_mode",
                "stride_mode" => "dma_stride_mode",
                "idx_mode" => "dma_idx_mode",
                "precision" if c.name.contains("OFM") => "ofm_precision",
                "precision" => "ifm_precision",
                "format" => "data_format",
                other => other,
            };
            let symbol = opcodes::symbol(u85, kind, *v)
                .map(|s| format!(" · {s}"))
                .unwrap_or_default();
            decoded
                .fields
                .push((name.replace('_', " "), format!("{v} (0x{v:x}){symbol}")));
        }
        if !decoded.fields.is_empty() {
            sections.push(decoded);
        }
        if c.name.contains("DMA") {
            let mut dma = self.dma_sections(index, &state);
            dma.insert(0, descriptions::instruction_section(c, u85, &state));
            dma.extend(sections.into_iter().map(InspectionSection::collapsed));
            return dma;
        }
        for (prefix, title) in [
            ("IFM", "Input tensor · IFM"),
            ("IFM2", "Second input · IFM2"),
            ("OFM", "Output tensor · OFM"),
        ] {
            let chained =
                u85 && get(&format!("{prefix}_PRECISION")).is_some_and(|p| p >> 14 & 3 == 2);
            let dims = if chained && prefix != "OFM" {
                get(&format!("{prefix}_REGION"))
                    .and_then(|b| chains.get(&b).copied())
                    .unwrap_or([None; 3])
            } else {
                let suffix = if prefix == "OFM" {
                    ["HEIGHT_M1", "WIDTH_M1", "DEPTH_M1"]
                } else {
                    ["HEIGHT0_M1", "WIDTH0_M1", "DEPTH_M1"]
                };
                suffix.map(|s| get(&format!("{prefix}_{s}")).map(|n| n + 1))
            };
            let mut dims =
                dims.map(|d| d.map(|d| d.to_string()).unwrap_or_else(|| "Unknown".into()));
            if prefix == "IFM2" && !chained {
                dims[2] = match get("IFM2_BROADCAST") {
                    Some(n) if n & 4 != 0 => "1 · broadcast".into(),
                    Some(_) => get("IFM_DEPTH_M1")
                        .map(|n| (n + 1).to_string())
                        .unwrap_or_else(|| "Unknown".into()),
                    None => "Unknown".into(),
                };
            }
            let datatype = get(&format!("{prefix}_PRECISION"))
                .map(|p| {
                    precision(u85, prefix == "OFM", p)
                        .split(',')
                        .next()
                        .unwrap_or("Unknown")
                        .to_owned()
                })
                .unwrap_or_else(|| "Not configured".into());
            let mut tensor = InspectionSection::new(title)
                .field("Datatype", datatype)
                .field("Height", &dims[0])
                .field("Width", &dims[1])
                .field("Channels", &dims[2])
                .field(
                    "Storage",
                    if chained {
                        "Internal chaining buffer"
                    } else {
                        "External base-pointer region"
                    },
                );
            if chained {
                tensor.fields.push((
                    "Address".into(),
                    address(prefix, &format!("{prefix}_BASE0")),
                ));
            }
            for tile in 0..4 {
                let register = format!("{prefix}_BASE{tile}");
                if !chained && get(&register).is_some() {
                    tensor
                        .fields
                        .push((format!("Tile {tile}"), address(prefix, &register)));
                }
            }
            for axis in ["X", "Y", "C"] {
                let register = format!("{prefix}_STRIDE_{axis}");
                if get(&register).is_some() {
                    tensor.fields.push((
                        format!("Stride {axis}"),
                        format!("{} bytes", value(&register)),
                    ));
                }
            }
            tensor.fields.push((
                "Shape scope".into(),
                if chained && prefix != "OFM" {
                    "Last preceding chain producer in linear stream order"
                } else {
                    "Configured NPU stripe/tile, not necessarily the full model tensor"
                }
                .into(),
            ));
            if prefix == "IFM2" {
                tensor.fields.push((
                    "Used by".into(),
                    "Binary elementwise operations when configured".into(),
                ));
            }
            sections.push(tensor);
        }
        for prefix in ["WEIGHT", "WEIGHT1", "WEIGHT2", "WEIGHT3", "SCALE", "SCALE1"] {
            if get(&format!("{prefix}_BASE")).is_some() {
                sections.push(
                    InspectionSection::new(prefix)
                        .field(
                            "Address",
                            address(
                                if prefix.starts_with("WEIGHT") {
                                    "WEIGHT"
                                } else {
                                    "SCALE"
                                },
                                &format!("{prefix}_BASE"),
                            ),
                        )
                        .field(
                            "Length",
                            format!("{} bytes", value(&format!("{prefix}_LENGTH"))),
                        ),
                );
            }
        }
        sections.insert(0, descriptions::instruction_section(c, u85, &state));
        sections
    }

    pub fn memory_ranges(&self) -> Vec<MemoryRange> {
        let mut ranges = vec![];
        for (kind, ios) in [("Input", &self.inputs), ("Output", &self.outputs)] {
            for (i, io) in ios
                .iter()
                .enumerate()
                .filter(|(_, io)| io.region >= 0 && io.offset >= 0)
            {
                let length = io
                    .shape
                    .iter()
                    .fold(u64::try_from(io.element_bytes).ok(), |n, d| {
                        n.and_then(|n| u64::try_from(*d).ok().and_then(|d| n.checked_mul(d)))
                    });
                ranges.push(MemoryRange {
                    arena: None,
                    region: io.region as u64,
                    internal: false,
                    offset: io.offset as u64,
                    length,
                    label: format!("{kind} {i}"),
                    transfer: vec![],
                    command: None,
                    note: format!("Logical tensor byte span · shape {:?}", io.shape),
                });
            }
        }
        let mut state: BTreeMap<&str, u64> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for (index, c) in self.instructions.iter().enumerate() {
            if c.name.starts_with("NPU_SET_") {
                state.insert(&c.name, c.value);
            }
            if c.name == "NPU_OP_DMA_START" {
                let transfer = [("DMA0_SRC", "Source"), ("DMA0_DST", "Destination")]
                    .into_iter()
                    .map(|(prefix, label)| (label.to_owned(), self.dma_address(prefix, &state)))
                    .collect::<Vec<_>>();
                for (prefix, role) in [("DMA0_SRC", "DMA source"), ("DMA0_DST", "DMA destination")]
                {
                    if let Some(mut range) = self.dma_range(prefix, role, index, &state) {
                        range.transfer = transfer.clone();
                        ranges.push(range);
                    }
                }
            }
            if matches!(
                c.name.as_str(),
                "NPU_OP_CONV"
                    | "NPU_OP_DEPTHWISE"
                    | "NPU_OP_POOL"
                    | "NPU_OP_ELEMENTWISE"
                    | "NPU_OP_RESIZE"
            ) {
                for prefix in ["WEIGHT", "WEIGHT1", "WEIGHT2", "WEIGHT3", "SCALE", "SCALE1"] {
                    let role = if prefix.starts_with("WEIGHT") {
                        "WEIGHT"
                    } else {
                        "SCALE"
                    };
                    let (Some(&region), Some(&offset)) = (
                        state.get(format!("NPU_SET_{role}_REGION").as_str()),
                        state.get(format!("NPU_SET_{prefix}_BASE").as_str()),
                    ) else {
                        continue;
                    };
                    let length = state
                        .get(format!("NPU_SET_{prefix}_LENGTH").as_str())
                        .copied();
                    if seen.insert((region & 7, offset, length, prefix)) {
                        ranges.push(MemoryRange { arena: None, region: region & 7, internal: false, offset, length, label: prefix.into(), command: Some(index), transfer: vec![], note: "Recorded buffer base and length; allocations may overlap across commands.".into() });
                    }
                }
            }
        }
        ranges.sort_by_key(|r| (r.space(), r.offset));
        ranges
    }
}
fn precision(u85: bool, ofm: bool, p: u64) -> String {
    if u85 {
        let bits = 8 << ((p >> 2) & 3);
        format!(
            "{}int{bits}, format={}, storage={}",
            if p & 1 != 0 { "" } else { "u" },
            (p >> 6) & 3,
            (p >> 14) & 3
        )
    } else {
        let code = if ofm { p & 7 } else { p & 15 };
        match (ofm, code) {
            (_, 0) => "uint8",
            (_, 1) => "int8",
            (false, 4) | (true, 2) => "uint16",
            (false, 5) | (true, 3) => "int16",
            (false, 9) | (true, 5) => "int32",
            _ => return format!("precision 0x{p:x}"),
        }
        .into()
    }
}
