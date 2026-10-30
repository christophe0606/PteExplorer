use super::*;

pub fn region_role(region: u64) -> &'static str {
    match region {
        0 => "Weights / constants",
        1 => "Scratch / activations",
        2 => "Fast scratch / staging",
        _ => "Runtime-defined buffer",
    }
}

pub fn addressing_help() -> InspectionSection {
    InspectionSection::new("How buffers, NPU regions and driver settings relate")
        .field("NPU region", "A numbered address slot selected by Vela's command stream. The runtime supplies a buffer's base address for that same slot.")
        .field("Base pointer (BASEP)", "The starting address of that buffer. BASEP[n] is this application's shorthand for the complete base address of NPU region n; hardware registers can split it into low and high words.")
        .field("Address calculation", "Runtime base address of NPU region n + byte offset. Offsets here are within the buffer, not offsets in the PTE file.")
        .field("Driver configuration", "Match NPU region n to NPU_REGIONCFG_n in your driver build. Its value configures the memory access path; it is not a buffer address or another region index. Encodings depend on U55, U65 or U85.")
        .field("Example", "NPU_REGIONCFG_0 = 2 still configures region 0. It does not change a command's region 0 into region 2.")
        .field("Arena and scratch", "A scratch buffer may be allocated within your runtime's tensor arena. An ExecuTorch arena ID is a separate identifier from an NPU region index.")
        .field("Available information", "Buffer roles below use the usual Vela mapping. Confirm it against your runtime integration. Physical addresses, SRAM/DRAM placement and driver REGIONCFG values are not stored in this PTE.")
        .collapsed()
}

fn bytes(n: u64) -> String {
    format!("{n} bytes (0x{n:x})")
}
fn get(state: &RegisterState<'_>, suffix: &str) -> Option<u64> {
    state.get(format!("NPU_SET_{suffix}").as_str()).copied()
}
fn buffer_name(region: Option<u64>) -> String {
    match region {
        Some(n) if n & 0x100 != 0 => "Internal NPU memory".into(),
        Some(n) => format!("{} · NPU region {}", region_role(n & 7), n & 7),
        None => "Buffer not configured".into(),
    }
}

impl Vela {
    pub fn region_section(&self, region: u64) -> InspectionSection {
        let mut section =
            InspectionSection::new(format!("NPU region {region} · {}", region_role(region)))
                .field("Buffer role", region_role(region))
                .field(
                    "Role evidence",
                    if region <= 2 {
                        "Usual Vela assignment; confirm against your runtime's base-address setup"
                    } else {
                        "No fixed role assumed; use the recorded I/O and command uses below"
                    },
                )
                .field(
                    "Runtime base address",
                    format!("BASEP[{region}] · supplied at inference time; not recorded"),
                )
                .field(
                    "Driver setting to match",
                    format!("NPU_REGIONCFG_{region} · build-time value not recorded"),
                );
        let area = match region {
            0 => Some("const_mem_area"),
            1 => Some("arena_mem_area"),
            2 => Some("cache_mem_area · separate only when the memory mode uses fast scratch"),
            _ => None,
        };
        if let Some(area) = area {
            section = section.field("Vela memory-mode setting (usual mapping)", area);
        }
        if let Some((name, size)) = match region {
            0 => self
                .blocks
                .iter()
                .find(|b| b.name == "weight_data")
                .map(|b| ("weight_data", b.size as u64)),
            1 => self
                .scratch_size
                .map(|size| ("scratch_size", u64::from(size))),
            _ => None,
        } {
            section = section.field(
                "Container buffer (standard region mapping)",
                format!("{name} · {}", bytes(size)),
            );
        }
        for (kind, ios) in [("Input", &self.inputs), ("Output", &self.outputs)] {
            for (i, io) in ios
                .iter()
                .enumerate()
                .filter(|(_, io)| io.region >= 0 && io.region as u64 == region)
            {
                section = section.field(
                    format!("Recorded {kind} {i}"),
                    format!(
                        "shape {:?} · {} bytes/element · buffer offset {}",
                        io.shape, io.element_bytes, io.offset
                    ),
                );
            }
        }
        section.field("Physical memory / AXI path", "Not recorded; correlate your runtime base address, linker placement and driver configuration")
    }

    pub(super) fn dma_address(&self, prefix: &str, state: &RegisterState<'_>) -> String {
        let offset = get(state, prefix)
            .map(bytes)
            .unwrap_or_else(|| "Not configured".into());
        format!(
            "{} · offset {offset}",
            buffer_name(get(state, &format!("{prefix}_REGION")))
        )
    }

    pub(super) fn dma_range(
        &self,
        prefix: &str,
        label: &str,
        index: usize,
        state: &RegisterState<'_>,
    ) -> Option<MemoryRange> {
        let region = get(state, &format!("{prefix}_REGION"))?;
        Some(MemoryRange {
            arena: None,
            region: region & 7,
            internal: region & 0x100 != 0,
            offset: get(state, prefix)?,
            length: get(state, "DMA0_LEN"),
            label: label.into(),
            command: Some(index),
            transfer: vec![],
            note: "Configured byte span per transfer, before striding/indexing. Repeated or indexed accesses can touch other addresses. This is a snapshot of settings at the selected command, not a runtime trace.".into(),
        })
    }

    fn dma_endpoint(
        &self,
        prefix: &str,
        title: &str,
        index: usize,
        state: &RegisterState<'_>,
    ) -> InspectionSection {
        let region = get(state, &format!("{prefix}_REGION"));
        let mut section = InspectionSection::new(title).field("Buffer", buffer_name(region));
        let address = self.dma_address(prefix, state);
        if let Some(target) = self.dma_range(
            prefix,
            if prefix.ends_with("SRC") {
                "DMA source"
            } else {
                "DMA destination"
            },
            index,
            state,
        ) {
            section =
                section.memory_link("Buffer address · double-click to locate", address, target);
        } else {
            section = section.field("Buffer address", address);
        }
        match region {
            Some(n) if n & 0x100 != 0 => section
                .field("Address space", "Internal NPU memory, selected explicitly by the DMA command")
                .field("Driver region setting", "Not applicable to this internal-memory endpoint; no external BASEP slot is used"),
            Some(n) => {
                let n = n & 7;
                section = section.field("NPU region", n)
                    .field("Buffer role evidence", if n <= 2 { "Usual Vela mapping; not a physical memory identification" } else { "Runtime-defined; no role inferred from the number" })
                    .field("Driver setting to match", format!("NPU_REGIONCFG_{n} · value not recorded in the PTE"))
                    .field("Address calculation", format!("Runtime base of NPU region {n} (BASEP[{n}]) + byte offset"));
                if let Some((name, size)) = match n {
                    0 => self.blocks.iter().find(|b| b.name == "weight_data").map(|b| ("weight_data", b.size as u64)),
                    1 => self.scratch_size.map(|n| ("scratch_size", u64::from(n))),
                    _ => None,
                } {
                    section = section.field("Container buffer (standard region mapping)", format!("{name} · {}", bytes(size)));
                }
                section
            }
            None => section.field("NPU region", "Not configured at this command; no region is assumed"),
        }
    }

    pub(super) fn dma_sections(
        &self,
        index: usize,
        state: &RegisterState<'_>,
    ) -> Vec<InspectionSection> {
        let c = &self.instructions[index];
        let status = match c.name.as_str() {
            "NPU_OP_DMA_START" => "Launches a DMA copy using the settings below",
            "NPU_OP_DMA_WAIT" => {
                "Waits for DMA completion; the settings below are the current DMA state, not a new copy"
            }
            _ => {
                "Updates DMA settings; this command does not launch a copy. NPU_OP_DMA_START launches it"
            }
        };
        let summary = InspectionSection::new("Memory transfer")
            .field("Direction", format!("{} → {}", buffer_name(get(state, "DMA0_SRC_REGION")), buffer_name(get(state, "DMA0_DST_REGION"))))
            .field("Length per transfer", get(state, "DMA0_LEN").map(bytes).unwrap_or_else(|| "Not configured".into()))
            .field("At this command", status)
            .field("Data", "Raw bytes; may contain encoded weights or other data. Tensor datatype and shape are not generally encoded by DMA")
            .field("Repeated / indexed copies", "Length is for one transfer. Stride and index settings can change which addresses are accessed; see advanced settings");
        let mut advanced =
            InspectionSection::new("Advanced DMA settings · strides and indexing").collapsed();
        if self
            .config
            .as_ref()
            .is_some_and(|h| h.architecture == "Ethos-U85")
        {
            if let Some(control) = get(state, "DMA0_SRC_REGION") {
                advanced = advanced.field(
                    "Source layout",
                    match (control >> 9) & 3 {
                        0 => "1D · one contiguous span per transfer",
                        1 => "2D · repeated spans using stride 0",
                        2 => "3D · repeated spans using strides 0 and 1",
                        _ => "Reserved / unknown layout",
                    },
                );
            }
            for (suffix, label) in [
                ("DMA0_SRC_REGION", "Source indexing"),
                ("DMA0_DST_REGION", "Destination indexing"),
            ] {
                if let Some(control) = get(state, suffix) {
                    advanced = advanced.field(
                        label,
                        if control & 0x800 != 0 {
                            "Enabled · addresses depend on index-table contents"
                        } else {
                            "Disabled"
                        },
                    );
                }
            }
        }
        for (suffix, label) in [
            ("DMA0_SRC_REGION", "Source region / mode control"),
            ("DMA0_DST_REGION", "Destination region / mode control"),
            ("DMA0_SIZE0", "Repeat size 0"),
            ("DMA0_SIZE1", "Repeat size 1"),
            ("DMA0_SRC_STRIDE0", "Source stride 0 · bytes"),
            ("DMA0_SRC_STRIDE1", "Source stride 1 · bytes"),
            ("DMA0_DST_STRIDE0", "Destination stride 0 · bytes"),
            ("DMA0_DST_STRIDE1", "Destination stride 1 · bytes"),
            ("DMA0_IDX_REGION", "Index-table NPU region"),
            ("DMA0_IDX", "Index-table offset · bytes"),
            ("DMA0_IDX_MAX", "Index maximum"),
        ] {
            if let Some(n) = get(state, suffix) {
                advanced = advanced.field(format!("{label} ({suffix})"), format!("{n} (0x{n:x})"));
            }
        }
        vec![
            summary,
            self.dma_endpoint("DMA0_SRC", "Source · read from", index, state),
            self.dma_endpoint("DMA0_DST", "Destination · write to", index, state),
            addressing_help(),
            advanced,
        ]
    }
}
