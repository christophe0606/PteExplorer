//! Plain-language instruction help. Semantics follow Vela's command generators.
use super::*;

enum ValueKind {
    Bytes,
    Offset(String),
    Region(String),
    DmaRegion,
    Count(String),
    MinusOne(String),
    Fields,
    KernelStride,
    Precision(bool),
    Scalar,
    Wait,
}
struct Guide {
    action: String,
    purpose: String,
    kind: ValueKind,
    execution: &'static str,
    related: String,
}
impl Guide {
    fn setting(
        action: impl Into<String>,
        purpose: impl Into<String>,
        kind: ValueKind,
        related: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            purpose: purpose.into(),
            kind,
            related: related.into(),
            execution: "Stores a setting used by subsequent operations until overwritten. This instruction does not start a kernel or a DMA copy.",
        }
    }
    fn operation(
        action: &str,
        purpose: &str,
        kind: ValueKind,
        related: &str,
        execution: &'static str,
    ) -> Self {
        Self {
            action: action.into(),
            purpose: purpose.into(),
            kind,
            related: related.into(),
            execution,
        }
    }
}

fn guide(name: &str) -> Option<Guide> {
    use ValueKind::*;
    let launch = "Starts the configured operation. Work may overlap other queued work; a later wait can synchronize completion.";
    let sync = "Synchronizes previously submitted work. This instruction does not launch new work or change buffer addresses.";
    let control = "Controls command execution or measurement; it does not configure tensor dimensions or launch a compute kernel.";
    let op = match name {
        "NPU_OP_CONV" => Guide::operation(
            "Start convolution",
            "Reads the first input tensor (IFM) and weights, computes convolution, then applies configured scaling and activation to produce the output tensor (OFM). On U85, weights_ifm2 can select the second input as weights.",
            Fields,
            "IFM / OFM settings; KERNEL_*; WEIGHT_*; SCALE_*; ACTIVATION_*",
            launch,
        ),
        "NPU_OP_DEPTHWISE" => Guide::operation(
            "Start depthwise convolution",
            "Applies a separate convolution filter to each input channel, using configured weights and quantization, and writes the output tensor (OFM).",
            Fields,
            "IFM / OFM settings; KERNEL_*; WEIGHT_*; SCALE_*",
            launch,
        ),
        "NPU_OP_POOL" => Guide::operation(
            "Start pooling or reduction",
            "Combines input values over the configured kernel window to produce an output tensor. The operation parameter selects the pooling/reduction function, such as maximum or average.",
            Fields,
            "IFM / OFM settings; KERNEL_*; OFM_SCALE",
            launch,
        ),
        "NPU_OP_ELEMENTWISE" => Guide::operation(
            "Start an elementwise operation",
            "Applies the selected arithmetic, comparison or other elementwise function to input values. Depending on the function, it uses IFM alone, IFM with IFM2, or a scalar operand, and produces OFM.",
            Fields,
            "IFM / IFM2 / OFM settings; *_BROADCAST; scalar and scale settings",
            launch,
        ),
        "NPU_OP_RESIZE" => Guide::operation(
            "Start image/tensor resizing",
            "Samples the input tensor at coordinates defined by the resize scale, step and offset settings, and writes the resized output tensor. The mode selects the sampling function.",
            Fields,
            "IFM / OFM settings; RESIZE_X_*; RESIZE_Y_*",
            launch,
        ),
        "NPU_OP_DMA_START" => Guide::operation(
            "Start the configured DMA copy",
            "Reads bytes from the configured source and writes them to the configured destination. SRC_REGION / DST_REGION select the buffers, SRC / DST give starting byte offsets, and LEN gives bytes per transfer. Repetition and indexing can add further accesses.",
            Fields,
            "DMA0_SRC_REGION + DMA0_SRC; DMA0_DST_REGION + DMA0_DST; DMA0_LEN; repeat/stride/index settings",
            launch,
        ),
        "NPU_OP_DMA_WAIT" => Guide::operation(
            "Wait for earlier DMA copies",
            "Pauses command processing until outstanding DMA work meets the encoded completion threshold. A zero threshold waits for all earlier copies in the selected DMA queue. The DMA state shown below does not describe a new copy.",
            Wait,
            "Earlier NPU_OP_DMA_START commands",
            sync,
        ),
        "NPU_OP_KERNEL_WAIT" => Guide::operation(
            "Wait for earlier compute kernels",
            "Pauses command processing until outstanding compute work meets the encoded completion threshold. A zero threshold waits for all earlier kernels in the selected compute queue.",
            Wait,
            "Earlier convolution, depthwise, pooling, elementwise or resize operations",
            sync,
        ),
        "NPU_OP_STOP" => Guide::operation(
            "End the command stream",
            "Terminates command-stream execution using the encoded stop control/mask. This is an execution-control instruction rather than a tensor setting or a new operation.",
            Fields,
            "Outstanding-work synchronization and completion/interrupt handling",
            control,
        ),
        "NPU_OP_IRQ" => Guide::operation(
            "Request an interrupt",
            "Signals the interrupt request selected by the encoded mask so host software can be notified. It does not copy data or compute a tensor.",
            Fields,
            "Host driver's interrupt and completion handling",
            control,
        ),
        "NPU_OP_PMU_MASK" => Guide::operation(
            "Control performance measurement",
            "Controls whether command execution contributes to performance-monitor measurements. The encoded enable/control value affects measurement, rather than tensor values or memory placement.",
            Fields,
            "Performance Monitoring Unit (PMU) counters",
            control,
        ),
        "NPU_OP_BRANCH" => Guide::operation(
            "Branch to another command-stream location",
            "Changes command execution to the encoded target when the branch condition is satisfied. The state below follows linear file order and cannot determine which branch runs at inference time.",
            Fields,
            "branch_cond and branch_target fields; command-stream control flow",
            control,
        ),
        _ => {
            if let Some(suffix) = name.strip_prefix("NPU_SET_DMA0_") {
                return dma_guide(suffix);
            }
            if let Some(suffix) = name.strip_prefix("NPU_SET_") {
                return setting_guide(suffix);
            }
            return None;
        }
    };
    Some(op)
}

fn dma_guide(suffix: &str) -> Option<Guide> {
    use ValueKind::*;
    let (action, purpose, kind, related) = match suffix {
        "SRC_REGION" => (
            "Select the DMA source buffer",
            "Selects the NPU region whose runtime base address will be used for DMA reads, or selects internal NPU memory. Additional mode bits control striding/indexing where supported. The region index chooses which buffer; it does not set a byte offset or a transfer length.",
            DmaRegion,
            "DMA0_SRC sets the starting byte offset; DMA0_LEN sets bytes per transfer; NPU_OP_DMA_START launches the copy",
        ),
        "DST_REGION" => (
            "Select the DMA destination buffer",
            "Selects the NPU region whose runtime base address will be used for DMA writes, or selects internal NPU memory. Additional mode bits control access modes where supported. The region index chooses which buffer; it does not set a byte offset or a transfer length.",
            DmaRegion,
            "DMA0_DST sets the starting byte offset; DMA0_LEN sets bytes per transfer; NPU_OP_DMA_START launches the copy",
        ),
        "SRC" => (
            "Set the DMA source start offset",
            "Sets where reading starts, in bytes from the base of the buffer selected by DMA0_SRC_REGION. This defines the start of the source span, not its length. DMA0_LEN supplies the bytes per transfer; stride/index settings can modify later accesses.",
            Offset("DMA0_SRC".into()),
            "Source address = runtime base selected by DMA0_SRC_REGION + DMA0_SRC. Copy length comes from DMA0_LEN",
        ),
        "DST" => (
            "Set the DMA destination start offset",
            "Sets where writing starts, in bytes from the base of the buffer selected by DMA0_DST_REGION. This defines the start of the destination span, not its length. DMA0_LEN supplies the bytes per transfer; stride/index settings can modify later accesses.",
            Offset("DMA0_DST".into()),
            "Destination address = runtime base selected by DMA0_DST_REGION + DMA0_DST. Copy length comes from DMA0_LEN",
        ),
        "LEN" => (
            "Set the DMA bytes per transfer",
            "Sets how many bytes are copied in one transfer, starting at the configured source and destination offsets. Repeated/strided/indexed mode can perform multiple transfers, so this value alone need not be the total bytes moved.",
            Bytes,
            "DMA0_SRC_REGION / DMA0_SRC; DMA0_DST_REGION / DMA0_DST; SIZE, stride and index settings",
        ),
        "SIZE0" => (
            "Set the first DMA repetition size",
            "Sets the size of the first repetition dimension for a multidimensional DMA copy. It controls repetition together with the selected DMA mode and stride settings; DMA0_LEN remains the bytes per individual transfer.",
            Count("repetition-size units (dimension 0)".into()),
            "DMA0_SRC_REGION mode bits; DMA0_LEN; stride 0 settings",
        ),
        "SIZE1" => (
            "Set the second DMA repetition size",
            "Sets the size of the outer repetition dimension for a multidimensional DMA copy. It is used with the selected DMA mode and outer stride settings, rather than selecting another memory region.",
            Count("repetition-size units (dimension 1)".into()),
            "DMA0_SRC_REGION mode bits; DMA0_SIZE0; stride 1 settings",
        ),
        "SRC_STRIDE0" => (
            "Set source spacing for DMA repetition 0",
            "Sets the source byte stride used when the first repetition dimension advances. This changes the spacing between source accesses, while DMA0_SRC supplies the initial offset and DMA0_LEN supplies each transfer's length.",
            Bytes,
            "DMA0_SIZE0; DMA0_SRC_REGION stride mode; DMA0_SRC",
        ),
        "SRC_STRIDE1" => (
            "Set source spacing for DMA repetition 1",
            "Sets the source byte stride used when the outer repetition dimension advances in a 3D DMA copy. It is a spacing value, not a source region index or a copy length.",
            Bytes,
            "DMA0_SIZE1; DMA0_SRC_REGION stride mode; DMA0_SRC",
        ),
        "DST_STRIDE0" => (
            "Set destination spacing for DMA repetition 0",
            "Sets the destination byte stride used when the first repetition dimension advances. It changes the spacing between writes; DMA0_DST remains the initial offset.",
            Bytes,
            "DMA0_SIZE0; DMA0_DST; selected DMA repetition mode",
        ),
        "DST_STRIDE1" => (
            "Set destination spacing for DMA repetition 1",
            "Sets the destination byte stride used when the outer repetition dimension advances in a 3D DMA copy. It changes write spacing without changing the bytes per transfer.",
            Bytes,
            "DMA0_SIZE1; DMA0_DST; selected DMA repetition mode",
        ),
        "SKIP0" => (
            "Set legacy DMA skip control 0",
            "Sets the first legacy DMA skip setting used for repeated address advancement. It participates in mode-dependent addressing, rather than selecting a buffer or setting the initial source offset.",
            Fields,
            "DMA0_SIZE0; DMA0_SRC_REGION / DMA0_DST_REGION access modes",
        ),
        "SKIP1" => (
            "Set legacy DMA skip control 1",
            "Sets the second legacy DMA skip setting used for repeated address advancement. Interpret its encoded value with the DMA repetition mode, not as the copy length or a region index.",
            Fields,
            "DMA0_SIZE1; DMA0_SRC_REGION / DMA0_DST_REGION access modes",
        ),
        "IDX_REGION" => (
            "Select the DMA index-table buffer",
            "Selects the external NPU region containing the index table used by indexed DMA. This selects the table's buffer, rather than the source or destination data buffer.",
            Region("DMA0_IDX".into()),
            "DMA0_IDX supplies the table's byte offset; endpoint region controls enable indexing",
        ),
        "IDX" => (
            "Set the DMA index-table start offset",
            "Sets the byte offset of the index table within DMA0_IDX_REGION. The table's entries affect source or destination addresses when indexing is enabled; this is not the start offset of the data being copied.",
            Offset("DMA0_IDX".into()),
            "DMA0_IDX_REGION; DMA0_SRC_REGION / DMA0_DST_REGION indexing mode",
        ),
        "IDX_MAX" => (
            "Set the DMA index limit",
            "Sets the maximum-index control used by indexed DMA addressing. It constrains indexing rather than specifying bytes per transfer. Interpret the limit together with the selected indexing mode.",
            Fields,
            "DMA0_IDX_REGION; DMA0_IDX; indexed source/destination controls",
        ),
        "IDX_SKIP1" => (
            "Set the outer DMA index-table skip",
            "Sets how index-table traversal skips between outer groups in 3D indexed DMA. It affects which index entries are consumed, rather than the length of a data copy.",
            Fields,
            "DMA0_SIZE1; DMA0_IDX; 3D stride and indexing modes",
        ),
        _ => return None,
    };
    Some(Guide::setting(action, purpose, kind, related))
}

fn setting_guide(suffix: &str) -> Option<Guide> {
    use ValueKind::*;
    for (prefix, operand) in [
        ("IFM2", "second input tensor (IFM2)"),
        ("IFM", "first input tensor (IFM)"),
        ("OFM", "output tensor (OFM)"),
    ] {
        if let Some(part) = suffix.strip_prefix(&format!("{prefix}_")) {
            let related = format!(
                "{prefix}_REGION, {prefix}_BASE0–3, {prefix}_PRECISION and tensor dimensions; the subsequent compute operation"
            );
            let (action, purpose, kind) = match part {
                "REGION" => (format!("Select storage for the {operand}"), format!("Selects the external NPU region used to address the {operand}. On U85 with internal chaining selected by PRECISION, the value instead identifies a chain buffer. BASE0–3 supply offsets only when external storage is used."), Region(prefix.into())),
                "PRECISION" => (format!("Set datatype and storage for the {operand}"), format!("Specifies how values of the {operand} are represented and accessed: datatype/precision and the encoded layout, scaling/rounding or storage options supported by this architecture. These are packed fields, not a tensor size or an address."), Precision(prefix == "OFM")),
                "ZERO_POINT" => (format!("Set the quantization zero point for the {operand}"), format!("Sets the integer zero point associated with the {operand}. Quantized values are interpreted relative to this value; it is not a memory offset. Signed interpretation depends on the configured datatype."), Scalar),
                "BROADCAST" => (format!("Set broadcasting for the {operand}"), format!("Selects which dimensions of the {operand} are reused across output positions during elementwise work, and any scalar/operand-order options encoded here. Broadcasting changes how input values are reused, not the allocation size."), Fields),
                "SCALE" => (format!("Set arithmetic scaling for the {operand}"), format!("Sets the multiplier/shift and available scaling controls for the {operand}. This changes numerical conversion during computation; it does not resize the tensor or move it in memory."), Fields),
                "SCALAR" => (format!("Set a scalar value for the {operand}"), "Sets a quantized scalar operand used in place of reading a full second tensor when scalar operation is configured. Its datatype comes from the operand precision setting.".into(), Scalar),
                "UPSCALE" => ("Select input upsampling".into(), "Selects how the first input is expanded/resampled before the configured kernel consumes it. The mode determines whether and how new sample positions are supplied; it is not a byte stride.".into(), Fields),
                "IB_END" => ("Set the end of the internal input buffer".into(), "Sets the boundary of the internal shared-RAM allocation used for the first input. This is an NPU working-buffer layout setting, not an external NPU region index or a tensor-arena offset.".into(), Count("internal buffer boundary index".into())),
                "IB_START" => ("Set the start of the second internal input buffer".into(), "Sets the internal shared-RAM boundary where storage for the second input begins. It separates internal working buffers; it does not set the external address of IFM2.".into(), Count("internal buffer boundary index".into())),
                _ => {
                    if let Some(tile) = part.strip_prefix("BASE").filter(|n| matches!(*n, "0" | "1" | "2" | "3")) {
                        (format!("Set tile {tile}'s start offset for the {operand}"), format!("Sets the starting byte offset of tile {tile} relative to {prefix}_REGION's runtime base address. The tile's extent comes from dimensions and strides, not from this offset. External tile addresses are not used when U85 internal chaining supplies the tensor."), Offset(prefix.into()))
                    } else if let Some(axis) = part.strip_prefix("STRIDE_").filter(|a| matches!(*a, "X" | "Y" | "C")) {
                        let step = match axis { "X" => "one column", "Y" => "one row", _ => "one channel or channel block, according to the layout" };
                        (format!("Set {axis}-direction byte spacing for the {operand}"), format!("Sets how far the address advances when moving {step} through the {operand}. A byte stride describes storage spacing; it does not set the tensor's dimension or its base address."), Bytes)
                    } else if let Some(edge) = part.strip_prefix("PAD_").filter(|e| matches!(*e, "TOP" | "LEFT" | "RIGHT" | "BOTTOM")) {
                        (format!("Set {} padding for the input", edge.to_lowercase()), format!("Sets the number of padded sample positions on the {} edge of the input consumed by the kernel. Padding supplies boundary values without adding a separately addressed tensor buffer.", edge.to_lowercase()), Count("padded sample positions".into()))
                    } else {
                        let dimension = part.strip_suffix("_M1")?;
                        let (what, unit) = match dimension {
                            "WIDTH" => ("stripe width", "columns"), "HEIGHT" => ("stripe height", "rows"), "DEPTH" => ("stripe channel count", "channels"),
                            "WIDTH0" => ("first tile-column width", "columns"), "HEIGHT0" => ("first tile-row height", "rows"), "HEIGHT1" => ("second tile-row height", "rows"),
                            "BLK_WIDTH" => ("compute-block width", "columns per compute block"), "BLK_HEIGHT" => ("compute-block height", "rows per compute block"), "BLK_DEPTH" => ("compute-block channel count", "channels per compute block"),
                            _ => return None,
                        };
                        (format!("Set {what} for the {operand}"), format!("Sets the {what} used while processing the {operand}. The stored number is one less than the actual dimension, so the decoded size is value + 1. This is a configured stripe/tile/block size, not necessarily the full model tensor size."), MinusOne(unit.into()))
                    }
                }
            };
            return Some(Guide::setting(action, purpose, kind, related));
        }
    }
    for prefix in ["WEIGHT", "WEIGHT1", "WEIGHT2", "WEIGHT3", "SCALE", "SCALE1"] {
        if let Some(part) = suffix.strip_prefix(&format!("{prefix}_")) {
            let weights = prefix.starts_with("WEIGHT");
            let buffer = if weights {
                "encoded weights"
            } else {
                "per-channel scale/bias data"
            };
            let region = if weights { "WEIGHT" } else { "SCALE" };
            let (action, purpose, kind) = match part {
                "REGION" => (format!("Select the buffer containing {buffer}"), format!("Selects the NPU region used to read {buffer}. The associated BASE offsets and LENGTH values identify the byte spans within that buffer; this command does not contain an address or a byte length."), Region(region.into())),
                "BASE" => (format!("Set the start offset of {prefix} data"), format!("Sets the byte offset at which {buffer} for {prefix} starts within {region}_REGION. {prefix}_LENGTH supplies the byte length. This is a buffer-relative offset, not a physical address."), Offset(region.into())),
                "LENGTH" => (format!("Set the byte length of {prefix} data"), format!("Sets the length of the {buffer} span for {prefix}, in bytes starting at {prefix}_BASE. This is the encoded storage size, not the number or dimensions of logical weights/channels."), Bytes),
                "FORMAT" if weights => ("Select how weights are encoded".into(), "Selects the encoded weight format and sparsity options used by the weight decoder. It describes how stored weight bytes are interpreted, not where the bytes are placed.".into(), Fields),
                _ => return None,
            };
            return Some(Guide::setting(
                action,
                purpose,
                kind,
                format!(
                    "{region}_REGION; {prefix}_BASE; {prefix}_LENGTH; convolution/depthwise operations"
                ),
            ));
        }
    }
    let (action, purpose, kind, related) = match suffix {
        "KERNEL_WIDTH_M1" => (
            "Set the kernel's horizontal extent",
            "Sets the effective kernel width encoded as width minus one. For convolution, dilation can enlarge this extent relative to the number of filter taps. The horizontal kernel stride controls how the window moves across the input.",
            MinusOne("sample positions".into()),
            "KERNEL_STRIDE; IFM padding; convolution/pooling operation",
        ),
        "KERNEL_HEIGHT_M1" => (
            "Set the kernel's vertical extent",
            "Sets the effective kernel height encoded as height minus one. For convolution, dilation can enlarge this extent relative to the number of filter taps. The vertical kernel stride controls how the window moves down the input.",
            MinusOne("sample positions".into()),
            "KERNEL_STRIDE; IFM padding; convolution/pooling operation",
        ),
        "KERNEL_STRIDE" => (
            "Set kernel movement and dilation",
            "Sets the packed horizontal/vertical kernel-stride, dilation and available traversal/decomposition controls. Kernel stride moves the sampling window in sample positions; it is different from IFM_STRIDE_X/Y, which are byte spacings in memory.",
            KernelStride,
            "KERNEL_WIDTH_M1 / HEIGHT_M1; IFM padding; IFM_STRIDE_X/Y",
        ),
        "ACC_FORMAT" => (
            "Set accumulator representation",
            "Selects the internal accumulator format used to hold intermediate arithmetic results. Available fields can also control accumulator reuse/input/output. This is working arithmetic precision, not the input or output tensor datatype.",
            Fields,
            "Compute operation; input/output precision; output scale",
        ),
        "ACTIVATION" => (
            "Select the output activation function",
            "Selects the activation or lookup-table function applied to computed output values, plus available table/clip options. Separate MIN and MAX commands set the quantized clipping bounds.",
            Fields,
            "ACTIVATION_MIN; ACTIVATION_MAX; OFM_PRECISION",
        ),
        "ACTIVATION_MIN" => (
            "Set the lower output clipping bound",
            "Sets the lower bound used when clamping activated output values. This is a quantized numerical bound whose signed interpretation depends on output precision, not a tensor dimension or byte offset.",
            Scalar,
            "ACTIVATION; ACTIVATION_MAX; OFM_PRECISION",
        ),
        "ACTIVATION_MAX" => (
            "Set the upper output clipping bound",
            "Sets the upper bound used when clamping activated output values. This is a quantized numerical bound whose signed interpretation depends on output precision, not a buffer limit.",
            Scalar,
            "ACTIVATION; ACTIVATION_MIN; OFM_PRECISION",
        ),
        "AB_START" => (
            "Set the internal accumulator-buffer start",
            "Sets the shared-RAM boundary where the accumulator working buffer begins. Together with internal input-buffer boundaries it partitions NPU working memory; it does not identify an external arena or NPU base-pointer region.",
            Count("internal buffer boundary index".into()),
            "IFM_IB_END; IFM2_IB_START; ACC_FORMAT; compute-block settings",
        ),
        "BLOCKDEP" => (
            "Set dependence on the preceding compute operation",
            "Sets the block-dependency control used when scheduling this operation against the preceding compute operation. It allows safe overlap while respecting producer/consumer data hazards; it is not a block dimension or a byte count.",
            Fields,
            "Preceding and following compute operations; KERNEL_WAIT for explicit synchronization",
        ),
        "PARALLEL_MODE" => (
            "Select parallel execution capacity",
            "Sets the parallel execution mode used by subsequent compute operations. Vela encodes the configured core count minus one here; this does not change the number of tensor channels.",
            MinusOne("execution cores".into()),
            "Target NPU configuration; subsequent compute operations",
        ),
        "OPA_SCALE" => (
            "Set scaling for elementwise operand A",
            "Sets the multiplier and encoded shift applied to the first elementwise operand before operands are combined. This is arithmetic scaling, not a spatial resize or a byte address.",
            Fields,
            "OPB_SCALE; OFM_SCALE; NPU_OP_ELEMENTWISE",
        ),
        "OPB_SCALE" => (
            "Set scaling for elementwise operand B",
            "Sets arithmetic scaling for the second elementwise operand before operands are combined. Interpret the encoded scale together with operand-A and output scaling settings.",
            Fields,
            "OPA_SCALE; OFM_SCALE; NPU_OP_ELEMENTWISE",
        ),
        "OP_SCALAR" => (
            "Set the scalar compute operand",
            "Sets the quantized scalar supplied when a compute operation selects scalar broadcasting. The operand datatype and broadcast settings determine how the scalar bits are interpreted.",
            Scalar,
            "IFM_BROADCAST / IFM2_BROADCAST; operand precision; compute operation",
        ),
        _ => {
            if let Some(rest) = suffix.strip_prefix("RESIZE_") {
                let axis = if rest.starts_with('X') {
                    "horizontal"
                } else if rest.starts_with('Y') {
                    "vertical"
                } else {
                    return None;
                };
                let (action, purpose, kind) = if rest.ends_with("SCALE_N_M1") {
                    (
                        format!("Set the {axis} resize scale numerator"),
                        format!(
                            "Sets the numerator N used in the {axis} rational resize coordinate calculation, encoded as N minus one. Combine it with step and offset settings; this value alone is not the full resize ratio or an output dimension."
                        ),
                        MinusOne("scale-numerator units".into()),
                    )
                } else if rest.ends_with("OFFSET") {
                    (
                        format!("Set the {axis} resize coordinate offset"),
                        format!(
                            "Sets the initial {axis} sampling-coordinate offset for resizing. Interpret this encoded coordinate value together with the rational scale and step settings; it is not an offset into a memory buffer."
                        ),
                        Fields,
                    )
                } else if matches!(rest, "X" | "Y" | "X_STEP" | "Y_STEP") {
                    (
                        format!("Set the {axis} resize coordinate steps"),
                        format!(
                            "Sets integer and remainder increments for advancing {axis} sampling coordinates by one output position and by a compute block. These are packed coordinate steps, not byte strides or a standalone resize ratio."
                        ),
                        Fields,
                    )
                } else {
                    return None;
                };
                return Some(Guide::setting(
                    action,
                    purpose,
                    kind,
                    "RESIZE_*_SCALE_N_M1; RESIZE_*_OFFSET; RESIZE_*_STEP; NPU_OP_RESIZE",
                ));
            }
            return None;
        }
    };
    Some(Guide::setting(action, purpose, kind, related))
}

fn fields_value(c: &Command, u85: bool) -> String {
    if c.fields.is_empty() {
        return if c.value == 0 {
            "No additional value operand".into()
        } else {
            format!("Encoded control: {} (0x{:x})", c.value, c.value)
        };
    }
    c.fields
        .iter()
        .map(|(name, value)| {
            let kind = match name.as_str() {
                "mode" if c.name == "NPU_OP_POOL" => "pooling_mode",
                "mode" if c.name == "NPU_OP_ELEMENTWISE" => "elementwise_mode",
                "mode" if c.name == "NPU_SET_IFM_UPSCALE" => "ifm_upscale_mode",
                "precision" if c.name.contains("OFM") => "ofm_precision",
                "precision" => "ifm_precision",
                "format" => "data_format",
                "region_mode" => "dma_region_mode",
                "stride_mode" => "dma_stride_mode",
                "idx_mode" => "dma_idx_mode",
                "round_mode" if c.name.contains("OFM") => "round_mode_ofm",
                "round_mode" => "round_mode_ifm",
                "dilation_x" | "dilation_y" => "kernel_dilation",
                "decomposition" => "kernel_decomposition",
                other => other,
            };
            let symbol = opcodes::symbol(u85, kind, *value)
                .map(|s| format!(" · {s}"))
                .unwrap_or_default();
            let label = if name == "data" && c.name.ends_with("_SCALE") {
                "scale".into()
            } else {
                name.replace('_', " ")
            };
            format!("{label} = {value}{symbol}")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn interpreted_value(guide: &Guide, c: &Command, u85: bool, state: &RegisterState<'_>) -> String {
    let get = |suffix: &str| state.get(format!("NPU_SET_{suffix}").as_str()).copied();
    let n = c.value;
    match &guide.kind {
        ValueKind::Bytes => format!("{n} bytes (0x{n:x})"),
        ValueKind::Count(unit) => format!("{unit}: {n} (encoded 0x{n:x})"),
        ValueKind::MinusOne(unit) => format!(
            "{} {unit} · decoded as {n} + 1 (encoded value {n}, 0x{n:x})",
            n + 1
        ),
        ValueKind::Scalar => format!(
            "Encoded quantized value: {n} (0x{n:x}); signedness and interpretation follow the configured datatype"
        ),
        ValueKind::Precision(output) => {
            let datatype = precision(u85, *output, n);
            format!(
                "{}; {}",
                datatype.split(',').next().unwrap_or(&datatype),
                fields_value(c, u85)
            )
        }
        ValueKind::Fields => fields_value(c, u85),
        ValueKind::KernelStride => {
            let extension_mask = if u85 { 1 } else { 7 };
            let x = 1 + (n & 1) + (((n >> 6) & extension_mask) << 1);
            let y = 1 + ((n >> 1) & 1) + (((n >> 9) & extension_mask) << 1);
            let dilation_x = 1 + ((n >> 3) & 1);
            let dilation_y = 1 + ((n >> 4) & 1);
            let order = if n & 4 == 0 {
                "depth first"
            } else {
                "part kernel first"
            };
            let decomposition = if u85 {
                if n & 32 == 0 {
                    "; decomposition 8 x 8"
                } else {
                    "; decomposition 4 x 4"
                }
            } else {
                ""
            };
            format!(
                "Window movement: X = {x}, Y = {y} sample positions; dilation: X = {dilation_x}, Y = {dilation_y}; weight traversal: {order}{decomposition}"
            )
        }
        ValueKind::Wait => {
            let threshold = c
                .fields
                .get("k")
                .or_else(|| c.fields.get("n"))
                .copied()
                .unwrap_or(n & 15);
            format!(
                "Outstanding-work threshold {threshold}{}",
                if threshold == 0 {
                    " · waits for all earlier work in the selected queue"
                } else {
                    " · permits the encoded amount of work to remain outstanding"
                }
            )
        }
        ValueKind::DmaRegion => {
            if n & 0x100 != 0 {
                format!("Internal NPU memory selected; {}", fields_value(c, u85))
            } else {
                format!(
                    "NPU region {} · {} (usual Vela role); {}",
                    n & 7,
                    region_role(n & 7),
                    fields_value(c, u85)
                )
            }
        }
        ValueKind::Region(prefix) => {
            if u85 && get(&format!("{prefix}_PRECISION")).is_some_and(|p| p >> 14 & 3 == 2) {
                format!(
                    "Internal chain buffer {n}; current storage mode does not select an external NPU region"
                )
            } else {
                format!(
                    "NPU region {} · {} (usual Vela role); runtime supplies this region's base address",
                    n & 7,
                    region_role(n & 7)
                )
            }
        }
        ValueKind::Offset(prefix) => {
            let base = match get(&format!("{prefix}_REGION")) {
                _ if u85
                    && get(&format!("{prefix}_PRECISION")).is_some_and(|p| p >> 14 & 3 == 2) =>
                {
                    return format!(
                        "Encoded offset {n} bytes (0x{n:x}); current internal-chain storage does not use external tile offsets"
                    );
                }
                Some(r) if prefix.starts_with("DMA") && r & 0x100 != 0 => {
                    "the internal NPU memory address space".into()
                }
                Some(r) => format!("the runtime base of NPU region {}", r & 7),
                None => "the selected buffer's base (region not yet configured)".into(),
            };
            format!("{n} bytes (0x{n:x}) from {base}")
        }
    }
}

pub(super) fn explain(name: &str, _u85: bool) -> String {
    guide(name).map(|g| format!("{} {}", g.purpose, g.execution)).unwrap_or_else(|| "This opcode is not recognized for this architecture. Its function is unavailable; raw instruction data is retained.".into())
}

pub(super) fn instruction_section(
    c: &Command,
    u85: bool,
    state: &RegisterState<'_>,
) -> InspectionSection {
    let Some(guide) = guide(&c.name) else {
        return InspectionSection::new("Selected instruction")
            .field("Function", "Unknown instruction")
            .field("Meaning", explain(&c.name, u85));
    };
    InspectionSection::new("Selected instruction")
        .field("Function", &guide.action)
        .field("Meaning", &guide.purpose)
        .field(
            if c.name.starts_with("NPU_SET_") {
                "Value written"
            } else {
                "Operation parameter"
            },
            interpreted_value(&guide, c, u85, state),
        )
        .field("Execution", guide.execution)
        .field("Used with", guide.related)
        .field("State shown below", "The remaining cards show accumulated settings in linear command-stream order, including values written by earlier instructions.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_supported_opcode_has_specific_user_help() {
        let mut names = BTreeSet::new();
        for u85 in [false, true] {
            for payload in [false, true] {
                for code in 0..=0x3ff {
                    if let Some(name) = opcodes::name(u85, payload, code) {
                        let help = guide(name).unwrap_or_else(|| panic!("Missing help for {name}"));
                        assert!(
                            help.purpose.len() > 60,
                            "Insufficient explanation for {name}"
                        );
                        assert!(!help.related.is_empty());
                        names.insert(name);
                    }
                }
            }
        }
        assert!(names.len() > 100);
        assert!(guide("NPU_SET_UNRECOGNIZED").is_none());
    }
}
