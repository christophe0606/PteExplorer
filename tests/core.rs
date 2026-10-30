use pte_explorer::{
    graph::{Camera, Graph, Selection},
    pte::Program,
    vela::Vela,
};
#[path = "support/mixed.rs"]
mod mixed;

#[test]
fn mixed_kernel_graph_uses_schemas_and_shared_tensors() {
    use pte_explorer::graph::{ArgumentRole, argument_roles, arguments};
    let m = mixed::method();
    let graph = Graph::method(&m);
    for i in 0..m.instructions.len() {
        assert!(!argument_roles(&m, i).contains(&ArgumentRole::Unknown));
    }
    assert_eq!(graph.nodes.len(), 8); // six operators and method boundaries
    assert_eq!(graph.edges.len(), 7);
    assert!(
        graph
            .edges
            .iter()
            .all(|e| e.directed && graph.nodes[e.from].x < graph.nodes[e.to].x)
    );
    assert_eq!(arguments(&m, 4)[1].name, "weight");
    assert_eq!(arguments(&m, 4)[3].name, "stride");
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| !matches!(n.selection, Selection::Value(1..=7 | 9..=20)))
    );
    let conv = graph
        .nodes
        .iter()
        .find(|n| n.selection == Selection::Operator(4))
        .unwrap();
    assert_eq!(conv.title, "convolution");
    assert_eq!(conv.properties.len(), 2);
    assert_eq!(
        conv.properties[0],
        ("weight".into(), "float32 [32, 1, 3, 3]".into())
    );
    for id in [0, 8] {
        assert_eq!(
            graph
                .nodes
                .iter()
                .filter(|n| n.selection == Selection::Value(id))
                .count(),
            1,
            "Output alias v{id} was duplicated"
        );
    }
    let quantize = graph
        .nodes
        .iter()
        .position(|n| n.selection == Selection::Operator(0))
        .unwrap();
    let delegate = graph
        .nodes
        .iter()
        .position(|n| n.selection == Selection::Operator(1))
        .unwrap();
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.from == quantize && e.to == delegate)
    );
    // In-place operators read the preceding version before producing a new one.
    let mut inplace = mixed::method();
    inplace.instructions.truncate(1);
    inplace.instructions[0].name = "aten::relu_".into();
    inplace.instructions[0].args = vec![0, 0];
    inplace.outputs = vec![0];
    let graph = Graph::method(&inplace);
    assert_eq!(graph.nodes.len(), 3);
    assert!(
        graph
            .edges
            .iter()
            .all(|e| graph.nodes[e.from].x < graph.nodes[e.to].x)
    );
}

#[test]
fn yamnet_graph_regression_when_supplied() {
    use pte_explorer::graph::{ArgumentRole, argument_roles};
    let Some(path) = std::env::var_os("PTE_EXPLORER_YAMNET_MODEL") else {
        return;
    };
    let program = Program::open(std::path::Path::new(&path)).unwrap();
    let method = &program.methods[0];
    assert_eq!(method.instructions.len(), 84);
    assert_eq!(method.values.len(), 547);
    assert_eq!(method.delegates.len(), 14);
    for i in 0..method.instructions.len() {
        assert!(
            !argument_roles(method, i).contains(&ArgumentRole::Unknown),
            "Unclassified instruction {i}"
        );
    }
    let graph = Graph::method(method);
    assert_eq!(graph.nodes.len(), 86); // 84 operations and two method boundaries
    assert_eq!(graph.edges.len(), 85);
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| !matches!(n.selection, Selection::Value(id) if id != 28 && id != 540))
    );
    let arenas = pte_explorer::arena::ranges(method);
    assert_eq!(
        pte_explorer::arena::ids(method)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(pte_explorer::arena::capacity(method, 1), Some(807168));
    assert_eq!(arenas.len(), 86); // capacity and 85 tensors
    let tensor = arenas.iter().find(|r| r.label == "v29").unwrap();
    assert_eq!((tensor.offset, tensor.length), (24576, Some(12288)));
    assert!(
        graph
            .edges
            .iter()
            .all(|e| e.directed && !e.execution && graph.nodes[e.from].x < graph.nodes[e.to].x)
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .map(|n| n.properties.len())
            .sum::<usize>(),
        28
    );
    let mut reached = std::collections::BTreeSet::new();
    reached.insert(
        graph
            .nodes
            .iter()
            .position(|n| n.selection == Selection::Value(28))
            .unwrap(),
    );
    loop {
        let before = reached.len();
        for edge in &graph.edges {
            if reached.contains(&edge.from) {
                reached.insert(edge.to);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    assert_eq!(
        reached.len(),
        graph.nodes.len(),
        "The complete method must connect to its input"
    );
}

#[test]
fn arenas_distinguish_buffers_sizes_reuse_and_unknown_spans() {
    use pte_explorer::{arena, vela::MemorySpace};
    let mut method = mixed::method();
    method.memory_sizes.push(0x1_0000_1000);
    method.values[3].tensor.as_mut().unwrap().allocation = Some((2, 0x1_0000_0000));
    let ids = arena::ids(&method).into_iter().collect::<Vec<_>>();
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(arena::capacity(&method, 0), None);
    let ranges = arena::ranges(&method);
    let quantized = ranges.iter().find(|r| r.label == "v3").unwrap();
    assert_eq!(quantized.space(), MemorySpace::Arena(2));
    assert_eq!(quantized.length, Some(12288));
    assert_eq!(quantized.offset, 0x1_0000_0000);
    assert!(
        quantized
            .transfer
            .iter()
            .any(|(k, v)| k == "Bounds" && v.contains("exceeds"))
    );
    assert!(
        quantized
            .transfer
            .iter()
            .any(|(k, v)| k.starts_with("Operator uses")
                && v.contains("quantize_per_tensor")
                && v.contains("EthosUBackend"))
    );
    assert!(
        ranges
            .iter()
            .filter(|r| r.space() == MemorySpace::Arena(1) && r.offset == 0)
            .count()
            > 2
    );
    let tensor = method.values[3].tensor.as_mut().unwrap();
    tensor.shape_dynamism = 1;
    assert_eq!(arena::tensor_bytes(tensor), Some(12288));
    tensor.shape_dynamism = 2;
    assert_eq!(arena::tensor_bytes(tensor), None);
    tensor.shape_dynamism = 0;
    tensor.shape = vec![i32::MAX; 4];
    assert_eq!(arena::tensor_bytes(tensor), None);
    tensor.shape = vec![-1, 32];
    assert_eq!(arena::tensor_bytes(tensor), None);
    tensor.shape = vec![];
    assert_eq!(arena::tensor_bytes(tensor), Some(2));
    tensor.storage_offset = 1;
    assert_eq!(arena::tensor_bytes(tensor), None);
    let state = method.values[1].tensor.as_mut().unwrap();
    state.allocation = Some((1, 0));
    assert!(
        !pte_explorer::graph::constant_tensor(&method, 1),
        "Initialized mutable tensors must stay distinct from constants"
    );
}

fn words(xs: &[u32]) -> Vec<u8> {
    xs.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn block(dst: &mut Vec<u8>, name: &str, data: &[u8], external: bool) {
    let start = dst.len();
    dst.resize(start + 32, 0);
    dst[start..start + name.len()].copy_from_slice(name.as_bytes());
    dst[start + 16..start + 20].copy_from_slice(&(data.len() as u32).to_le_bytes());
    dst[start + 20] = external as u8;
    dst.extend(data);
    while !dst.len().is_multiple_of(16) {
        dst.push(0);
    }
}
fn payload(product: u32, commands: &[u32]) -> Vec<u8> {
    // Vela's COP1 framing: config action, two config words, NOPs, length tag.
    let mut xs = vec![
        u32::from_le_bytes(*b"COP1"),
        0x00100001,
        (product << 28) | 8,
        0x10000000,
        5,
        5,
        5,
        ((commands.len() as u32) << 16) | 2,
    ];
    xs.extend(commands);
    words(&xs)
}
fn container(product: u32, commands: &[u32]) -> Vec<u8> {
    let mut b = vec![];
    block(&mut b, "vela_bin_stream", &[], false);
    block(&mut b, "cmd_data", &payload(product, commands), false);
    block(&mut b, "vela_end_stream", &[], false);
    b
}
fn decode(product: u32, commands: &[u32]) -> Vela {
    Vela::parse(&container(product, commands), |_| {
        anyhow::bail!("not external")
    })
    .unwrap()
}

#[test]
fn three_architectures_and_architecture_specific_opcodes() {
    for (product, arch) in [(0, "Ethos-U55"), (1, "Ethos-U65"), (2, "Ethos-U85")] {
        let v = decode(
            product,
            &[0x0105 | (1 << 16), 0x010f | (1 << 16), 0x4000, 0x1234, 2, 0],
        );
        assert_eq!(v.config.as_ref().unwrap().architecture, arch);
        assert_eq!(v.instructions[0].name, "NPU_SET_IFM_PRECISION");
        assert_eq!(v.instructions[3].name, "NPU_OP_CONV");
        assert!(v.details(3).contains("BASEP[1] + 4660"));
    }
    assert_eq!(
        decode(0, &[0x4025, 123]).instructions[0].name,
        "NPU_SET_OPA_SCALE"
    );
    assert_eq!(
        decode(1, &[0x4025, 123]).instructions[0].name,
        "NPU_SET_OPA_SCALE"
    );
    assert_eq!(
        decode(2, &[0x4025, 123]).instructions[0].name,
        "NPU_SET_IFM_SCALE"
    );
    assert_eq!(
        decode(0, &[0x012d]).instructions[0].name,
        "NPU_SET_AB_START"
    );
    assert_eq!(
        decode(2, &[0x012d]).instructions[0].name,
        "NPU_SET_RESIZE_Y_OFFSET"
    );
}
#[test]
fn dma_addresses_and_state_are_selected_at_instruction_time() {
    for product in 0..=2 {
        let v = decode(
            product,
            &[
                0x0130,
                0x0131 | (1 << 16),
                0x4030 | (1 << 16),
                0x80,
                0x4031,
                0x200,
                0x4032,
                64,
                0x10,
                0x4032,
                128,
                0x10,
            ],
        );
        let first = v.details(5);
        assert!(first.contains("BASEP[0] + 4294967424"), "{first}");
        assert!(first.contains("BASEP[1] + 512"));
        assert!(first.contains("64 (0x40) bytes"));
        assert!(v.details(7).contains("128 (0x80) bytes"));
        assert!(!first.contains("128 (0x80) bytes"));
        assert!(v.regions().contains("Region 0"));
        assert!(v.regions().contains("Region 1"));
        let sections = v.instruction_sections(5);
        let transfer = sections
            .iter()
            .find(|s| s.title == "Memory transfer")
            .unwrap();
        assert!(transfer.fields.iter().any(|(k, v)| k == "Direction"
            && v.contains("Weights / constants")
            && v.contains("Scratch / activations")));
        assert!(
            transfer
                .fields
                .iter()
                .any(|(k, v)| k == "Length per transfer" && v.starts_with("64 "))
        );
        let source = sections
            .iter()
            .find(|s| s.title == "Source · read from")
            .unwrap();
        let destination = sections
            .iter()
            .find(|s| s.title == "Destination · write to")
            .unwrap();
        let src = source.memory_links.values().next().unwrap();
        let dst = destination.memory_links.values().next().unwrap();
        assert_eq!(
            (src.region, src.offset, src.length, src.command),
            (0, 4294967424, Some(64), Some(5))
        );
        assert_eq!((dst.region, dst.offset), (1, 512));
        assert!(
            source
                .fields
                .iter()
                .any(|(k, v)| k == "Driver setting to match" && v.contains("NPU_REGIONCFG_0"))
        );
        assert!(
            destination
                .fields
                .iter()
                .any(|(k, v)| k == "Address calculation" && v.contains("BASEP[1]"))
        );
        assert!(!sections.iter().any(|s| s.title.contains("Input tensor")));
        assert!(
            sections
                .iter()
                .find(|s| s.title == "Instruction")
                .unwrap()
                .collapsed
        );
        let ranges = v.memory_ranges();
        assert!(ranges.iter().any(|r| r.region == 0
            && r.offset == 4294967424
            && r.length == Some(64)
            && r.command == Some(5)));
        assert!(ranges.iter().any(|r| r.region == 1
            && r.offset == 512
            && r.length == Some(128)
            && r.command == Some(7)));
    }
}
#[test]
fn dma_internal_regions_missing_state_and_configuration_are_distinguished() {
    use pte_explorer::vela::MemorySpace;
    for product in 0..=2 {
        let incomplete = decode(product, &[0x10]);
        let sections = incomplete.instruction_sections(0);
        assert!(
            sections
                .iter()
                .flat_map(|s| &s.fields)
                .any(|(k, v)| k == "Length per transfer" && v == "Not configured")
        );
        assert!(sections.iter().all(|s| s.memory_links.is_empty()));
        let v = decode(
            product,
            &[
                0x0130 | (0x100 << 16),
                0x4030,
                0x80,
                0x0131 | (2 << 16),
                0x4031,
                0x400,
                0x4032,
                64,
                0x10,
                0x11,
            ],
        );
        let sections = v.instruction_sections(5);
        let src = sections
            .iter()
            .find(|s| s.title == "Source · read from")
            .unwrap();
        let dst = sections
            .iter()
            .find(|s| s.title == "Destination · write to")
            .unwrap();
        assert_eq!(
            src.memory_links.values().next().unwrap().space(),
            MemorySpace::Internal
        );
        assert!(
            src.fields
                .iter()
                .any(|(k, v)| k == "Driver region setting" && v.starts_with("Not applicable"))
        );
        assert_eq!(
            dst.memory_links.values().next().unwrap().space(),
            MemorySpace::Region(2)
        );
        assert!(
            dst.fields
                .iter()
                .any(|(k, v)| k == "Buffer" && v.contains("Fast scratch / staging"))
        );
        let ranges = v.memory_ranges();
        assert!(
            ranges
                .iter()
                .any(|r| r.internal && r.offset == 128 && r.length == Some(64))
        );
        assert!(!v.region_uses().contains_key(&0));
        let wait = v.instruction_sections(6);
        assert!(
            wait.iter()
                .find(|s| s.title == "Memory transfer")
                .unwrap()
                .fields
                .iter()
                .any(|(k, v)| k == "At this command" && v.contains("not a new copy"))
        );
        let staged = v.instruction_sections(4);
        assert!(
            staged
                .iter()
                .find(|s| s.title == "Memory transfer")
                .unwrap()
                .fields
                .iter()
                .any(|(k, v)| k == "At this command" && v.contains("does not launch"))
        );
        let unknown = v.region_section(7);
        assert!(
            unknown
                .fields
                .iter()
                .any(|(k, v)| k == "Buffer role" && v == "Runtime-defined buffer")
        );
    }
}
#[test]
fn selected_instruction_explains_region_offset_length_dimensions_and_scaling() {
    fn field<'a>(sections: &'a [pte_explorer::vela::InspectionSection], key: &str) -> &'a str {
        assert_eq!(sections[0].title, "Selected instruction");
        assert!(!sections[0].collapsed);
        &sections[0].fields.iter().find(|(k, _)| k == key).unwrap().1
    }
    for product in 0..=2 {
        let v = decode(
            product,
            &[
                0x0130,
                0x4030,
                208,
                0x4032,
                320,
                0x0104 | (15 << 16),
                0x4004,
                16,
                0x4024 | (5 << 16),
                123,
                0x122,
                0x10,
                0x12,
            ],
        );
        let region = v.instruction_sections(0);
        assert!(field(&region, "Function").contains("source buffer"));
        assert!(field(&region, "Value written").contains("NPU region 0"));
        let source = v.instruction_sections(1);
        assert!(field(&source, "Function").contains("start offset"));
        assert!(field(&source, "Value written").contains("208 bytes (0xd0)"));
        assert!(field(&source, "Meaning").contains("not its length"));
        assert!(field(&source, "Used with").contains("DMA0_LEN"));
        let length = v.instruction_sections(2);
        assert!(field(&length, "Value written").starts_with("320 bytes"));
        assert!(field(&length, "Function").contains("bytes per transfer"));
        let channels = v.instruction_sections(3);
        assert!(field(&channels, "Value written").starts_with("16 channels"));
        let byte_stride = v.instruction_sections(4);
        assert!(field(&byte_stride, "Value written").starts_with("16 bytes"));
        let scale = v.instruction_sections(5);
        assert!(field(&scale, "Meaning").contains("numerical conversion"));
        assert!(field(&scale, "Value written").contains("scale = 123"));
        assert!(field(&scale, "Value written").contains("shift = 5"));
        let kernel_stride = v.instruction_sections(6);
        assert!(field(&kernel_stride, "Meaning").contains("sample positions"));
        assert!(field(&kernel_stride, "Meaning").contains("byte spacings"));
        assert!(field(&kernel_stride, "Value written").contains("X = 1, Y = 1 sample positions"));
        let stride = decode(product, &[0x122 | ((1 | (1 << 9) | (1 << 3)) << 16)]);
        let stride_sections = stride.instruction_sections(0);
        assert!(field(&stride_sections, "Value written").contains("X = 2, Y = 3 sample positions"));
        assert!(field(&stride_sections, "Value written").contains("dilation: X = 2, Y = 1"));
        let start = v.instruction_sections(7);
        assert!(field(&start, "Execution").starts_with("Starts"));
        let wait = v.instruction_sections(8);
        assert!(field(&wait, "Operation parameter").contains("threshold 0"));
        assert!(field(&wait, "Execution").starts_with("Synchronizes"));
        for i in 0..=6 {
            let setting = v.instruction_sections(i);
            assert!(field(&setting, "Execution").contains("does not start"));
        }
        let unknown = decode(product, &[0x3ff]).instruction_sections(0);
        assert_eq!(field(&unknown, "Function"), "Unknown instruction");
    }
    // Resize enum names must resolve to the corresponding STEP field definitions.
    let resize = decode(2, &[0x4096 | (2 << 16), 3 | (4 << 16)]);
    assert_eq!(resize.instructions[0].name, "NPU_SET_RESIZE_X_STEP");
    assert_eq!(resize.instructions[0].fields["one_step_int"], 2);
    assert_eq!(resize.instructions[0].fields["one_step_mod"], 3);
    assert!(field(&resize.instruction_sections(0), "Value written").contains("blk step mod = 4"));
}
#[test]
fn dimensions_precision_and_incomplete_state_are_explicit() {
    let v = decode(
        2,
        &[
            0x010b | (7 << 16),
            0x010a | (8 << 16),
            0x0104 | (15 << 16),
            0x0105 | (5 << 16),
            0x0112 | (3 << 16),
            0x0111 | (4 << 16),
            0x0113 | (31 << 16),
            2,
        ],
    );
    let text = v.details(7);
    assert!(text.contains("H=8, W=9, C=16; int16"), "{text}");
    assert!(text.contains("H=4, W=5, C=32"));
    assert!(v.details(0).contains("W=?, C=?"));
    let sections = v.instruction_sections(7);
    let input = sections
        .iter()
        .find(|s| s.title == "Input tensor · IFM")
        .unwrap();
    for (key, expected) in [("Height", "8"), ("Width", "9"), ("Channels", "16")] {
        assert!(input.fields.iter().any(|(k, v)| k == key && v == expected));
    }
}
#[test]
fn u85_chaining_has_internal_addresses_and_producer_dimensions() {
    let v = decode(
        2,
        &[
            0x80000114,
            0x011f | (3 << 16),
            0x0112 | (3 << 16),
            0x0111 | (4 << 16),
            0x0113 | (7 << 16),
            2,
            0x80000105,
            0x010f | (3 << 16),
            0x0114,
            0x011f | (1 << 16),
            5,
        ],
    );
    let producer = v.details(5);
    assert!(producer.contains("OFM: internal chaining buffer 3"));
    assert!(producer.contains("H=4, W=5, C=8"));
    let consumer = v.details(10);
    assert!(consumer.contains("IFM: internal chaining buffer 3"));
    assert!(consumer.contains("H=4, W=5, C=8"));
    assert!(!v.regions().contains("Region 3"));
    let input = v
        .instruction_sections(10)
        .into_iter()
        .find(|s| s.title == "Input tensor · IFM")
        .unwrap();
    assert!(input.fields.iter().any(|(k, v)| k == "Height" && v == "4"));
    assert!(
        input
            .fields
            .iter()
            .any(|(k, v)| k == "Storage" && v == "Internal chaining buffer")
    );
    for product in 0..=2 {
        let v = decode(
            product,
            &[0x0104 | (15 << 16), 0x0180 | (4 << 16), 6 | (1 << 16)],
        );
        let text = v.details(2);
        assert!(text.contains("C=1 (channel broadcast)"));
        assert!(text.contains("ADD"), "{text}");
    }
}
#[test]
fn external_blocks_io_versions_and_truncation() {
    let cmd = payload(1, &[0]);
    let mut b = vec![];
    block(&mut b, "vela_bin_stream", &[], false);
    block(&mut b, "cmd_data", b"shared-command", true);
    block(
        &mut b,
        "inputs",
        &words(&[1, 1, 16, 16, 8, 1, 128, 1]),
        false,
    );
    block(
        &mut b,
        "outputs",
        &words(&[1, 1, 1, 1, 1, 8, 8, 16, 256, 1]),
        false,
    );
    block(&mut b, "vela_end_stream", &[], false);
    let v = Vela::parse(&b, |key| {
        assert_eq!(key, "shared-command");
        Ok(&cmd)
    })
    .unwrap();
    assert_eq!(v.inputs[0].shape, vec![1, 16, 16, 8]);
    assert_eq!(v.outputs[0].shape.len(), 6);
    assert_eq!(v.inputs[0].offset, 128);
    assert!(v.regions().contains("Physical base addresses"));
    for cut in 0..b.len() {
        assert!(
            Vela::parse(&b[..cut], |_| Ok(&cmd)).is_err(),
            "accepted truncation at {cut}"
        );
    }
    assert!(Vela::parse(&b, |_| anyhow::bail!("missing named data")).is_err());
    assert!(Vela::parse(&container(0, &[0x4000]), |_| unreachable!()).is_err());
}

// A minimal forward-writing FlatBuffer fixture assembler. The fixture uses
// explicit field slots from the upstream ET12 schema, independent of the reader.
struct Fb(Vec<u8>);
impl Fb {
    fn new() -> Self {
        let mut b = vec![0; 64];
        b[4..8].copy_from_slice(b"ET12");
        Self(b)
    }
    fn align(&mut self) {
        while !self.0.len().is_multiple_of(8) {
            self.0.push(0);
        }
    }
    fn u32(&mut self, p: usize, x: u32) {
        self.0[p..p + 4].copy_from_slice(&x.to_le_bytes());
    }
    fn link(&mut self, p: usize, d: usize) {
        self.u32(p, (d - p) as u32);
    }
    fn table(&mut self, n: usize) -> usize {
        self.align();
        let v = self.0.len();
        let len = 4 + 2 * n;
        self.0.extend((len as u16).to_le_bytes());
        self.0.extend(((8 + 8 * n) as u16).to_le_bytes());
        for i in 0..n {
            self.0.extend(((8 + 8 * i) as u16).to_le_bytes());
        }
        self.align();
        let t = self.0.len();
        self.0.resize(t + 8 + 8 * n, 0);
        self.u32(t, (t - v) as u32);
        t
    }
    fn field(t: usize, f: usize) -> usize {
        t + 8 + 8 * f
    }
    fn absent(&mut self, t: usize, f: usize) {
        let v = t - u32::from_le_bytes(self.0[t..t + 4].try_into().unwrap()) as usize;
        self.0[v + 4 + f * 2..v + 6 + f * 2].copy_from_slice(&[0, 0]);
    }
    fn vector(&mut self, items: &[u32]) -> usize {
        self.align();
        let p = self.0.len();
        self.0.extend(words(&[items.len() as u32]));
        self.0.extend(words(items));
        p
    }
    fn string(&mut self, s: &str) -> usize {
        self.align();
        let p = self.0.len();
        self.0.extend((s.len() as u32).to_le_bytes());
        self.0.extend(s.as_bytes());
        self.0.push(0);
        p
    }
    fn text(&mut self, t: usize, f: usize, s: &str) {
        let p = self.string(s);
        self.link(Self::field(t, f), p);
    }
    fn vecfield(&mut self, t: usize, f: usize, items: &[u32]) -> usize {
        let v = t - u32::from_le_bytes(self.0[t..t + 4].try_into().unwrap()) as usize;
        self.0[v + 4 + f * 2..v + 6 + f * 2].copy_from_slice(&((8 + 8 * f) as u16).to_le_bytes());
        let p = self.vector(items);
        self.link(Self::field(t, f), p);
        p
    }
}
fn fixture() -> Vec<u8> {
    let mut b = Fb::new();
    let root = b.table(8);
    b.u32(0, root as u32);
    for f in 2..8 {
        b.absent(root, f);
    }
    b.u32(Fb::field(root, 0), 42);
    let methods = b.vecfield(root, 1, &[0, 0]);
    for i in 0..2 {
        let m = b.table(9);
        b.link(methods + 4 + i * 4, m);
        b.text(m, 0, if i == 0 { "forward" } else { "get_config" });
        b.absent(m, 1);
        for f in 3..9 {
            b.absent(m, f);
        }
        let values = b.vecfield(m, 2, &[0]);
        let ev = b.table(2);
        b.link(values + 4, ev);
        b.u32(Fb::field(ev, 0), if i == 0 { 5 } else { 2 });
        let v = b.table(if i == 0 { 10 } else { 1 });
        b.link(Fb::field(ev, 1), v);
        if i == 0 {
            b.u32(Fb::field(v, 0), 6);
            b.u32(Fb::field(v, 1), 0);
            b.vecfield(v, 2, &[1, 3, 224, 224]);
            let order = b.string("\0\u{1}\u{2}\u{3}");
            b.link(Fb::field(v, 3), order);
            b.u32(Fb::field(v, 4), 0);
            b.u32(Fb::field(v, 5), 0);
            b.absent(v, 6);
            b.u32(Fb::field(v, 7), 0);
            b.u32(Fb::field(v, 8), 0);
            b.absent(v, 9);
            b.vecfield(m, 3, &[0]);
            b.vecfield(m, 4, &[0]);
        } else {
            b.u32(Fb::field(v, 0), 1234);
        }
    }
    b.0
}
#[test]
fn pte_multiple_methods_tensor_and_scalar_metadata() {
    let data = fixture();
    let p = Program::parse(&data, "fixture.pte").unwrap();
    assert_eq!(p.version, 42);
    assert_eq!(p.methods.len(), 2);
    let t = p.methods[0].values[0].tensor.as_ref().unwrap();
    assert_eq!(t.shape, vec![1, 3, 224, 224]);
    assert_eq!(t.dtype, "float32");
    assert_eq!(t.dim_order, vec![0, 1, 2, 3]);
    assert_eq!(p.methods[1].name, "get_config");
    assert_eq!(p.methods[1].values[0].description, "1234");
    let graph = Graph::method(&p.methods[0]);
    assert_eq!(graph.nodes[0].selection, Selection::Value(0));
    for cut in 0..data.len() {
        assert!(Program::parse(&data[..cut], "truncated").is_err());
    }
    let mut bad = data;
    bad[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Program::parse(&bad, "bad").is_err());
}
#[test]
fn segmented_pte_with_named_external_commands() {
    let cmd = payload(0, &[0x0105 | (1 << 16), 0]);
    let mut stream = vec![];
    block(&mut stream, "vela_bin_stream", &[], false);
    block(&mut stream, "cmd_data", b"commands", true);
    block(&mut stream, "vela_end_stream", &[], false);
    let mut b = Fb::new();
    let root = b.table(8);
    b.u32(0, root as u32);
    for f in [2, 3, 5, 6] {
        b.absent(root, f);
    }
    let methods = b.vecfield(root, 1, &[0]);
    let m = b.table(9);
    b.link(methods + 4, m);
    b.text(m, 0, "delegate_method");
    for f in 1..9 {
        b.absent(m, f);
    }
    let delegates = b.vecfield(m, 7, &[0]);
    let d = b.table(3);
    b.link(delegates + 4, d);
    b.text(d, 0, "EthosUBackend");
    b.absent(d, 2);
    let reference = b.table(2);
    b.link(Fb::field(d, 1), reference);
    b.u32(Fb::field(reference, 0), 1);
    b.u32(Fb::field(reference, 1), 0);
    let segments = b.vecfield(root, 4, &[0, 0]);
    for (i, (offset, size)) in [(0, stream.len()), (stream.len(), cmd.len())]
        .into_iter()
        .enumerate()
    {
        let s = b.table(2);
        b.link(segments + 4 + i * 4, s);
        b.0[Fb::field(s, 0)..Fb::field(s, 0) + 8].copy_from_slice(&(offset as u64).to_le_bytes());
        b.0[Fb::field(s, 1)..Fb::field(s, 1) + 8].copy_from_slice(&(size as u64).to_le_bytes());
    }
    let named = b.vecfield(root, 7, &[0]);
    let n = b.table(2);
    b.link(named + 4, n);
    b.text(n, 0, "commands");
    b.u32(Fb::field(n, 1), 1);
    b.align();
    let base = b.0.len();
    b.0[8..12].copy_from_slice(b"eh00");
    b.u32(12, 32);
    b.0[16..24].copy_from_slice(&(base as u64).to_le_bytes());
    b.0[24..32].copy_from_slice(&(base as u64).to_le_bytes());
    b.0.extend(stream);
    b.0.extend(cmd);
    let p = Program::parse(&b.0, "segmented fixture").unwrap();
    assert_eq!(p.segment_base, base as u64);
    assert_eq!(p.named_data["commands"], 1);
    let d = &p.methods[0].delegates[0];
    assert!(d.warning.is_none(), "{:?}", d.warning);
    let v = d.vela.as_ref().unwrap();
    assert_eq!(v.config.as_ref().unwrap().architecture, "Ethos-U55");
    assert_eq!(v.instructions.len(), 2);
    let end = b.0.len();
    assert!(Program::parse(&b.0[..end - 1], "truncated segment").is_err());
}
#[test]
fn zoom_anchor_fit_and_limits() {
    let mut c = Camera::default();
    let world = ((150. - c.x) / c.zoom, (120. - c.y) / c.zoom);
    c.zoom_at(2., (150., 120.));
    assert_eq!(
        (world.0 * c.zoom + c.x, world.1 * c.zoom + c.y),
        (150., 120.)
    );
    let g = Graph {
        width: 800.,
        height: 400.,
        ..Default::default()
    };
    c.fit(&g, 1000., 700.);
    assert!(g.width * c.zoom <= 960.001);
    assert!(g.height * c.zoom <= 660.001);
    c.zoom_at(999., (0., 0.));
    assert_eq!(c.zoom, 4.);
    c.zoom_at(0., (0., 0.));
    assert_eq!(c.zoom, 0.0001);
    let graph = Graph::method(&mixed::method());
    let node = graph
        .nodes
        .iter()
        .find(|n| n.selection == Selection::Operator(4))
        .unwrap();
    for (width, height) in [(150., 80.), (266., 200.), (600., 700.), (1300., 850.)] {
        for zoom in [0.1, 1., 4.] {
            let mut camera = Camera {
                x: -10000.,
                y: 1000.,
                zoom,
            };
            camera.reveal(node, width, height);
            assert!(camera.zoom <= zoom);
            assert!(camera.x + node.x * camera.zoom >= -0.01);
            assert!(camera.x + (node.x + node.w) * camera.zoom <= width + 0.01);
            assert!(camera.y + node.y * camera.zoom >= -0.01);
            assert!(camera.y + (node.y + node.h) * camera.zoom <= height + 0.01);
            let settled = camera;
            camera.reveal(node, width, height);
            assert!((camera.x - settled.x).abs() < 0.01 && (camera.y - settled.y).abs() < 0.01);
        }
    }
}
#[test]
fn local_model_regression_when_supplied() {
    let path = std::path::Path::new("model.pte");
    if !path.exists() {
        return;
    }
    let p = Program::open(path).unwrap();
    assert_eq!(p.methods[0].name, "forward");
    let v = p.methods[0].delegates[0].vela.as_ref().unwrap();
    assert!(!v.instructions.is_empty());
    assert!(v.warnings.is_empty(), "{:?}", v.warnings);
    let details = v.details(v.instructions.len() - 1);
    assert!(details.contains("Tensor state"));
}
