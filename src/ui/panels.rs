use super::*;
use pte_explorer::{
    graph::{ArgumentRole, argument_roles, arguments, constant_tensor, parameter},
    pte::{Method, Value},
    vela::{InspectionSection as Section, MemoryRange, MemorySpace, addressing_help, region_role},
};
use std::collections::BTreeSet;

pub(super) fn value_section(value: &Value, id: i32, role: &str) -> Section {
    let mut section = Section::new(format!("{role} · v{id}")).field("Value type", &value.kind);
    if let Some(t) = &value.tensor {
        section = section
            .field("Datatype", &t.dtype)
            .field("Dimensions", format!("{:?}", t.shape))
            .field("Dimension order", format!("{:?}", t.dim_order))
            .field("Storage offset", t.storage_offset)
            .field("Data buffer", t.data_buffer)
            .field(
                "Tensor data bytes",
                pte_explorer::arena::tensor_bytes(t)
                    .map(|n| format!("{n} bytes"))
                    .unwrap_or_else(|| "Not known from the recorded shape/datatype".into()),
            )
            .field(
                "Shape",
                match t.shape_dynamism {
                    0 => "Static",
                    1 => "Dynamic · bounded",
                    2 => "Dynamic · unbounded",
                    _ => "Unknown",
                },
            );
        if let Some(target) = pte_explorer::arena::tensor_range(value, id as usize) {
            let arena = target.arena.unwrap();
            section = section.memory_link("ExecuTorch arena", format!("Arena {arena} · byte offset {} (0x{:x}) · Click to view this tensor", target.offset, target.offset), target)
                .field("Address meaning", format!("Runtime base of planned buffer {arena} + this byte offset. The base address is not recorded. This arena ID is separate from the NPU region index."));
        } else {
            section = section.field(
                "Allocation",
                if t.data_buffer != 0 {
                    "Constant data; no planned arena allocation"
                } else {
                    "Runtime-provided buffer; no planned arena allocation recorded"
                },
            );
        }
        if !t.name.is_empty() {
            section = section.field("Name", &t.name);
        }
    } else {
        section = section.field("Value", &value.description);
    }
    if !value.references.is_empty() {
        section = section.field("Referenced values", format!("{:?}", value.references));
    }
    section
}
fn method_sections(m: &Method) -> Vec<Section> {
    vec![
        Section::new("Method")
            .field("Name", &m.name)
            .field("Instructions", m.instructions.len())
            .field("Inputs", format!("{:?}", m.inputs))
            .field("Outputs", format!("{:?}", m.outputs))
            .field("Planned memory (bytes)", format!("{:?}", m.memory_sizes))
            .field("Input structure", &m.input_structure)
            .field("Output structure", &m.output_structure),
        pte_explorer::arena::summary(m),
        pte_explorer::arena::help().collapsed(),
    ]
}
pub(super) fn explorer_document(
    p: Option<&Program>,
    method: usize,
    selection: Option<Selection>,
) -> (String, Vec<Section>) {
    let Some(p) = p else {
        return (
            "Welcome".into(),
            vec![Section::new("Open a model").field(
                "Start",
                "Use Open .pte… to inspect a local ExecuTorch model.",
            )],
        );
    };
    let Some(m) = p.methods.get(method) else {
        return (
            "File metadata".into(),
            vec![
                Section::new("File")
                    .field("Path", &p.path)
                    .field("Methods", p.methods.len()),
            ],
        );
    };
    match selection {
        Some(Selection::Value(i)) => (
            format!("Value v{i}"),
            vec![
                value_section(&m.values[i], i as i32, "Tensor / value"),
                pte_explorer::arena::summary(m),
                pte_explorer::arena::help().collapsed(),
            ],
        ),
        Some(Selection::Operator(i)) => {
            let op = &m.instructions[i];
            let args = arguments(m, i);
            let mut sections = vec![
                Section::new("Operator")
                    .field("Name", &op.name)
                    .field("Chain", op.chain)
                    .field("Instruction", op.index),
            ];
            let mut seen = BTreeSet::new();
            let mut constants = Vec::new();
            let mut parameters = Section::new("Parameters");
            for role in [
                ArgumentRole::Input,
                ArgumentRole::Output,
                ArgumentRole::Unknown,
            ] {
                for arg in args.iter().filter(|a| a.role == role) {
                    let value = &m.values[arg.value as usize];
                    if role != ArgumentRole::Output
                        && constant_tensor(m, arg.value)
                        && !m.inputs.contains(&arg.value)
                    {
                        constants.push(
                            value_section(value, arg.value, &format!("Constant · {}", arg.name))
                                .collapsed(),
                        );
                        continue;
                    }
                    if parameter(m, arg) {
                        parameters = parameters
                            .field(format!("{} · {}", arg.name, value.kind), &value.description);
                        continue;
                    }
                    if !seen.insert((arg.value, role as u8)) {
                        continue;
                    }
                    let label = match role {
                        ArgumentRole::Input => "Input",
                        ArgumentRole::Output => "Output",
                        ArgumentRole::Unknown => "Argument · direction unavailable",
                    };
                    sections.push(value_section(
                        value,
                        arg.value,
                        &format!("{label} · {}", arg.name),
                    ));
                }
            }
            if !parameters.fields.is_empty() {
                sections.push(parameters);
            }
            sections.extend(constants);
            if argument_roles(m, i).contains(&ArgumentRole::Unknown) {
                sections.push(Section::new("Direction information").field("Availability", "No matching operator schema is available for this kernel and argument count. Unclassified tensor/value ports use undirected links; all raw arguments remain available below."));
            }
            sections.push(Section::new("Raw call arguments").field("Value indices", format!("{:?}", op.args)).field("Output aliases", "An out buffer and its returned tensor can repeat the same value index. They are displayed once.").collapsed());
            sections.push(pte_explorer::arena::summary(m));
            sections.push(pte_explorer::arena::help().collapsed());
            if let Some(d) = op.delegate {
                for s in &m.delegates[d].specs {
                    sections.push(
                        Section::new(format!("Compile option · {}", s.key))
                            .field("Value", s.text.as_deref().unwrap_or(&s.hex)),
                    );
                }
            }
            (op.name.clone(), sections)
        }
        None => {
            let mut sections = vec![
                Section::new("File")
                    .field("Path", &p.path)
                    .field("Format", "ExecuTorch ET12")
                    .field("Schema version", p.version)
                    .field("Size", format!("{} bytes", p.file_size))
                    .field("Methods", p.methods.len()),
            ];
            sections.extend(method_sections(m));
            for (label, values) in [("Method input", &m.inputs), ("Method output", &m.outputs)] {
                for &id in values {
                    if let Some(v) = m.values.get(id as usize) {
                        sections.push(value_section(v, id, label));
                    }
                }
            }
            for (i, d) in m.delegates.iter().enumerate() {
                let mut s = Section::new(format!("Delegate {i}"))
                    .field("Backend", &d.id)
                    .field("Storage", &d.location)
                    .field("Size", format!("{} bytes", d.size));
                if let Some(v) = &d.vela {
                    if let Some(hw) = &v.config {
                        s = s
                            .field("Architecture", &hw.architecture)
                            .field("MACs / cycle", hw.macs_per_cycle);
                    }
                }
                for spec in &d.specs {
                    s = s.field(&spec.key, spec.text.as_deref().unwrap_or(&spec.hex));
                }
                if let Some(w) = &d.warning {
                    s = s.field("Warning", w);
                }
                sections.push(s);
            }
            if !p.segments.is_empty() {
                let mut s = Section::new("File segments")
                    .field("Segment base", format!("0x{:x}", p.segment_base));
                for (i, seg) in p.segments.iter().enumerate() {
                    s = s.field(
                        format!("Segment {i}"),
                        format!("0x{:x} · {} bytes", seg.offset, seg.size),
                    );
                }
                sections.push(s);
            }
            ("File & method metadata".into(), sections)
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct ScrollRail {
    pub handle: ScrollHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    drag: Rc<Cell<Option<f32>>>,
}
impl ScrollRail {
    pub fn new(handle: ScrollHandle) -> Self {
        Self {
            handle,
            ..Default::default()
        }
    }
    fn geometry(handle: &ScrollHandle, bounds: Bounds<Pixels>) -> (f32, f32, f32) {
        let height = f32::from(bounds.size.height).max(1.);
        let max = f32::from(handle.max_offset().height).max(0.);
        let thumb = (height * height / (height + max)).max(26.).min(height);
        let y = if max > 0. {
            -f32::from(handle.offset().y) / max * (height - thumb)
        } else {
            0.
        };
        (y, thumb, max)
    }
    fn move_thumb(handle: &ScrollHandle, bounds: Bounds<Pixels>, y: f32, grip: f32) {
        let (_, thumb, max) = Self::geometry(handle, bounds);
        let travel = (f32::from(bounds.size.height) - thumb).max(1.);
        let ratio = ((y - f32::from(bounds.origin.y) - grip) / travel).clamp(0., 1.);
        handle.set_offset(point(handle.offset().x, px(-max * ratio)));
    }
    pub fn render<V: 'static>(&self, id: &'static str, cx: &mut Context<V>) -> Stateful<Div> {
        let prepaint_bounds = self.bounds.clone();
        let handle = self.handle.clone();
        let down = self.clone();
        let movement = self.clone();
        let up = self.clone();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .relative()
            .w(px(16.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(0x101c29))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, e: &MouseDownEvent, _, cx| {
                    let b = down.bounds.get();
                    let (top, height, _) = Self::geometry(&down.handle, b);
                    let local = f32::from(e.position.y - b.origin.y);
                    let grip = if local >= top && local <= top + height {
                        local - top
                    } else {
                        height / 2.
                    };
                    down.drag.set(Some(grip));
                    Self::move_thumb(&down.handle, b, f32::from(e.position.y), grip);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |_, e: &MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    if let Some(grip) = movement.drag.get() {
                        Self::move_thumb(
                            &movement.handle,
                            movement.bounds.get(),
                            f32::from(e.position.y),
                            grip,
                        );
                        cx.notify();
                    }
                } else {
                    movement.drag.set(None);
                }
            }))
            .on_mouse_up(MouseButton::Left, move |_, _, _| up.drag.set(None))
            .on_scroll_wheel(cx.listener({
                let h = self.handle.clone();
                move |_, e: &ScrollWheelEvent, _, cx| {
                    let delta = e.delta.pixel_delta(px(24.));
                    let p = h.offset();
                    h.set_offset(point(p.x, p.y + delta.y));
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| prepaint_bounds.set(bounds),
                    move |bounds, _, window, _| {
                        let (top, height, _) = Self::geometry(&handle, bounds);
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin + point(px(3.), px(top)),
                                size(px(10.), px(height)),
                            ),
                            rgb(0x7694ab),
                        ));
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

pub(super) struct DetailPanel {
    pub title: String,
    pub sections: Vec<Section>,
    pub collapsed: BTreeSet<usize>,
    pub scroll: ScrollHandle,
    rail: ScrollRail,
    memory_ranges: Option<Arc<Vec<MemoryRange>>>,
    arena_ranges: Option<Arc<Vec<MemoryRange>>>,
    error: Option<String>,
}
impl DetailPanel {
    pub fn new(title: impl Into<String>, sections: Vec<Section>) -> Self {
        let scroll = ScrollHandle::new();
        let collapsed = sections
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.collapsed.then_some(i))
            .collect();
        Self {
            title: title.into(),
            sections,
            collapsed,
            rail: ScrollRail::new(scroll.clone()),
            scroll,
            memory_ranges: None,
            arena_ranges: None,
            error: None,
        }
    }
    pub fn with_memory_ranges(mut self, ranges: Arc<Vec<MemoryRange>>) -> Self {
        self.memory_ranges = Some(ranges);
        self
    }
    pub fn with_method(mut self, method: &Method) -> Self {
        self.arena_ranges = Some(Arc::new(pte_explorer::arena::ranges(method)));
        self
    }
    fn open_memory_window(&mut self, target: MemoryRange, cx: &mut Context<Self>) {
        let ranges = if target.arena.is_some() {
            &self.arena_ranges
        } else {
            &self.memory_ranges
        }
        .clone()
        .unwrap_or_else(|| Arc::new(vec![target.clone()]));
        let options = window_options(
            format!(
                "{} · {} · offset 0x{:x}",
                target.label,
                memory_label(target.space()),
                target.offset
            ),
            1250.,
            850.,
            cx,
        );
        if let Err(error) = cx.open_window(options, move |_, cx| {
            cx.new(|cx| MemoryWindow::new(ranges, target, cx))
        }) {
            self.error = Some(format!("Cannot open region map: {error:#}"));
            cx.notify();
        }
    }
    pub fn copy_text(&self) -> String {
        format!(
            "{}\n\n{}",
            self.title,
            self.sections
                .iter()
                .map(|s| format!(
                    "{}\n{}",
                    s.title,
                    s.fields
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ))
                .collect::<Vec<_>>()
                .join("\n\n")
        )
    }
}
impl Render for DetailPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .id("structured-details")
            .debug_selector(|| "structured-details".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_3()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()));
        for (i, section) in self.sections.iter().enumerate() {
            let collapsed = self.collapsed.contains(&i);
            let mut card = div()
                .mb_3()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(PANEL))
                .overflow_hidden()
                .child(
                    div()
                        .id(("section", i))
                        .debug_selector(move || format!("section-{i}"))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x233347))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.collapsed.remove(&i) {
                                this.collapsed.insert(i);
                            }
                            cx.notify();
                        }))
                        .child(if collapsed { "▸" } else { "▾" })
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .child(section.title.clone()),
                        ),
                );
            if !collapsed {
                let mut fields = div().p_3().flex().flex_col().gap_3();
                for (j, (label, value)) in section.fields.iter().enumerate() {
                    let mut field = div()
                        .id(("detail-field", i * 1000 + j))
                        .debug_selector(move || format!("field-{i}-{j}"))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(label.clone()),
                        )
                        .child(div().text_size(px(14.)).line_height(px(20.)).child(
                            if value.is_empty() {
                                "Not recorded".into()
                            } else {
                                value.clone()
                            },
                        ));
                    if let Some(target) = section.memory_links.get(&j) {
                        let target = target.clone();
                        field = field
                            .p_2()
                            .border_l_2()
                            .border_color(rgb(memory_color(target.space())))
                            .bg(rgb(0x233347))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x2b425b)))
                            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                                if event.click_count() == if target.arena.is_some() { 1 } else { 2 }
                                {
                                    this.open_memory_window(target.clone(), cx);
                                }
                            }));
                    }
                    fields = fields.child(field);
                }
                card = card.child(fields);
            }
            content = content.child(card);
        }
        let mut root = div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .font_family(system_font())
            .text_size(px(14.))
            .text_color(rgb(TEXT))
            .bg(rgb(BG));
        if let Some(error) = &self.error {
            root = root.child(div().p_2().bg(rgb(0x5c2830)).child(error.clone()));
        }
        root.child(
            div()
                .p_3()
                .text_size(px(17.))
                .font_weight(FontWeight::BOLD)
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(self.title.clone()),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(content)
                .child(self.rail.render("details-scrollbar", cx)),
        )
    }
}

pub(super) fn memory_color(space: MemorySpace) -> u32 {
    match space {
        MemorySpace::Region(n) => [
            0xf2ae66, 0x60d4b5, 0x72afff, 0xc39aff, 0xff98c0, 0xe4d36f, 0x6cdaed, 0xb5bfce,
        ][(n & 7) as usize],
        MemorySpace::Internal => 0xd8dbe2,
        MemorySpace::Arena(n) => [0x60d4b5, 0x72afff, 0xf2ae66, 0xc39aff][n as usize % 4],
    }
}
pub(super) fn memory_label(space: MemorySpace) -> String {
    match space {
        MemorySpace::Region(n) => format!("NPU region {n} · {}", region_role(n)),
        MemorySpace::Internal => "Internal NPU memory".into(),
        MemorySpace::Arena(n) => format!("ExecuTorch arena {n}"),
    }
}
pub(super) struct RegionMap {
    arena_view: bool,
    pub ranges: Arc<Vec<MemoryRange>>,
    regions: Vec<MemorySpace>,
    pub camera: Camera,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    drag: Option<Point<Pixels>>,
    moved: bool,
    pub selected: Option<usize>,
    pub detail: Entity<DetailPanel>,
    fit_pending: bool,
    focus_pending: bool,
    pub axis_start: u64,
    pub axis_span: u64,
}
impl RegionMap {
    pub fn new(ranges: Arc<Vec<MemoryRange>>, cx: &mut Context<Self>) -> Self {
        let regions = ranges
            .iter()
            .map(MemoryRange::space)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let axis_span = ranges
            .iter()
            .map(|r| r.offset.saturating_add(r.length.unwrap_or(1)))
            .max()
            .unwrap_or(1)
            .max(1);
        let arena_view = ranges.iter().any(|r| r.arena.is_some());
        let detail = if arena_view {
            cx.new(|_| DetailPanel::new("Arena view", vec![pte_explorer::arena::help()]))
        } else {
            cx.new(|_| DetailPanel::new("Region map", vec![Section::new("Read the map").field("Horizontal axis", "Byte offsets within each buffer. Each NPU region has its own color. Drag to pan; wheel or buttons to zoom.")
                .field("Bars", "Inputs, outputs, DMA transfers and weight/scale buffers. Click a bar for its source, offset and recorded length.")
                .field("Limits", "Observed spans may overlap or be reused; this is not an allocation or liveness map. Physical base addresses are selected by the runtime. Strided transfers may touch addresses beyond the displayed per-transfer span."), addressing_help() ]))
        };
        Self {
            arena_view,
            ranges,
            regions,
            camera: Camera::default(),
            viewport: Rc::new(Cell::new(Bounds::default())),
            drag: None,
            moved: false,
            selected: None,
            detail,
            fit_pending: true,
            focus_pending: false,
            axis_start: 0,
            axis_span,
        }
    }
    pub fn arenas(ranges: Arc<Vec<MemoryRange>>, cx: &mut Context<Self>) -> Self {
        let mut map = Self::new(ranges, cx);
        map.arena_view = true;
        map.detail = cx.new(|_| DetailPanel::new("Arena view", vec![pte_explorer::arena::help()]));
        map
    }
    pub fn focused(
        ranges: Arc<Vec<MemoryRange>>,
        target: MemoryRange,
        cx: &mut Context<Self>,
    ) -> Self {
        let same = |r: &MemoryRange| {
            r.space() == target.space()
                && r.offset == target.offset
                && r.length == target.length
                && r.command == target.command
                && r.label == target.label
        };
        let (ranges, index) = if let Some(i) = ranges.iter().position(same) {
            (ranges, i)
        } else {
            let mut ranges = (*ranges).clone();
            let index = ranges.len();
            ranges.push(target);
            (Arc::new(ranges), index)
        };
        let mut map = Self::new(ranges, cx);
        map.select_range(index, cx);
        map.focus_pending = true;
        map
    }
    fn select_range(&mut self, i: usize, cx: &mut Context<Self>) {
        self.selected = Some(i);
        let r = &self.ranges[i];
        if let Some(arena) = r.arena {
            let mut section = Section::new("Planned arena span")
                .field("ExecuTorch arena ID", arena)
                .field("Tensor / span", &r.label)
                .field(
                    "Byte offset in arena",
                    format!("{} bytes (0x{:x})", r.offset, r.offset),
                )
                .field(
                    "Data span length",
                    r.length
                        .map(|n| format!("{n} bytes (0x{n:x})"))
                        .unwrap_or_else(|| "Unknown; allocation offset marker".into()),
                )
                .field(
                    "Runtime address",
                    format!(
                        "Base of ExecuTorch arena {arena} + 0x{:x}; physical base not recorded",
                        r.offset
                    ),
                )
                .field("Interpretation", &r.note);
            section.fields.extend(r.transfer.clone());
            self.detail = cx.new(|_| {
                DetailPanel::new(r.label.clone(), vec![section, pte_explorer::arena::help()])
            });
            cx.notify();
            return;
        }
        let mut span = Section::new("Memory span")
            .field("Buffer / address space", memory_label(r.space()))
            .field("Source / use", &r.label)
            .field(
                "Offset in buffer",
                format!("{} bytes (0x{:x})", r.offset, r.offset),
            )
            .field(
                "Length per transfer / span",
                r.length
                    .map(|n| format!("{n} bytes (0x{n:x})"))
                    .unwrap_or_else(|| "Not configured · address marker".into()),
            )
            .field(
                "Command",
                r.command
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "Vela I/O descriptor".into()),
            )
            .field("Interpretation", &r.note);
        if !r.internal {
            span = span
                .field(
                    "Driver setting to match",
                    format!("NPU_REGIONCFG_{} · value not recorded", r.region),
                )
                .field(
                    "Runtime base address",
                    format!("BASEP[{}] · not recorded in the PTE", r.region),
                )
                .field(
                    "Role evidence",
                    "Usual Vela mapping for regions 0–2; other roles are runtime-defined",
                );
        }
        span.fields.extend(r.transfer.clone());
        self.detail = cx.new(|_| DetailPanel::new(r.label.clone(), vec![span, addressing_help()]));
        cx.notify();
    }
    fn row_y(&self, i: usize) -> f32 {
        let space = self.ranges[i].space();
        let group = self.regions.iter().position(|s| *s == space).unwrap();
        let prior = self.ranges.iter().filter(|r| r.space() < space).count();
        let within = self.ranges[..i]
            .iter()
            .filter(|r| r.space() == space)
            .count();
        72. + group as f32 * 72. + (prior + within) as f32 * 32.
    }
    pub fn focus_selected(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        let r = &self.ranges[i];
        let length = r.length.unwrap_or(256).max(1);
        self.axis_start = r.offset.saturating_sub((length / 4).max(1));
        self.axis_span = length.saturating_add((length / 2).max(2));
        let b = self.viewport.get();
        let zoom = ((f32::from(b.size.width) - 40.) / 1020.).clamp(0.25, 1.);
        let center = 180.
            + ((r.offset - self.axis_start) as f64 + r.length.unwrap_or(0) as f64 / 2.) * 800.
                / self.axis_span as f64;
        self.camera.zoom = zoom;
        // Leave a fixed label column beside the focused byte span.
        self.camera.x = (f32::from(b.size.width) + 130.) / 2. - center as f32 * zoom;
        self.camera.y = f32::from(b.size.height) / 2. - (self.row_y(i) + 12.) * zoom;
        self.fit_pending = false;
        self.focus_pending = false;
    }
    fn fit_arena(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        let space = self.ranges[i].space();
        self.axis_start = 0;
        self.axis_span = self
            .ranges
            .iter()
            .filter(|r| r.space() == space)
            .map(|r| r.offset.saturating_add(r.length.unwrap_or(1)))
            .max()
            .unwrap_or(1)
            .max(1);
        let b = self.viewport.get();
        self.camera.zoom = ((f32::from(b.size.width) - 40.) / 1020.).clamp(0.25, 1.);
        self.camera.x = 20.;
        self.camera.y = f32::from(b.size.height) / 2. - (self.row_y(i) + 12.) * self.camera.zoom;
        self.fit_pending = false;
        self.focus_pending = false;
    }
    fn dimensions(&self) -> (f32, f32) {
        (
            1020.,
            (64. + self.ranges.len() as f32 * 32. + self.regions.len() as f32 * 72.).max(200.),
        )
    }
    fn max_end(&self) -> u64 {
        self.ranges
            .iter()
            .map(|r| r.offset.saturating_add(r.length.unwrap_or(1)))
            .max()
            .unwrap_or(1)
            .max(1)
    }
    fn fit(&mut self) {
        self.axis_start = 0;
        self.axis_span = self.max_end();
        let (width, height) = self.dimensions();
        let b = self.viewport.get();
        self.camera.fit(
            &Graph {
                width,
                height,
                ..Default::default()
            },
            f32::from(b.size.width),
            f32::from(b.size.height),
        );
    }
    fn zoom(&mut self, zoom: f32) {
        let b = self.viewport.get();
        self.camera.zoom_at(
            zoom,
            (f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.),
        );
    }
}
impl Render for RegionMap {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let arena_view = self.arena_view;
        if self.fit_pending && self.viewport.get().size.width > px(0.) {
            if self.focus_pending {
                self.focus_selected();
            } else {
                self.fit();
            }
            self.fit_pending = false;
        }
        let bounds = self.viewport.clone();
        let view = cx.entity().downgrade();
        let mut area = div()
            .id("region-canvas")
            .debug_selector(|| "region-canvas".into())
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(0x0d1721))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    this.drag = Some(e.position);
                    this.moved = false;
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    if let Some(last) = this.drag {
                        let d = e.position - last;
                        if f32::from(d.x).abs() + f32::from(d.y).abs() > 1. {
                            this.moved = true;
                            this.camera.x += f32::from(d.x);
                            this.camera.y += f32::from(d.y);
                            cx.notify();
                        }
                        this.drag = Some(e.position);
                    }
                } else {
                    this.drag = None;
                }
            }))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                let local = e.position - this.viewport.get().origin;
                let d = e.delta.pixel_delta(px(24.));
                this.camera.zoom_at(
                    this.camera.zoom * (f32::from(d.y) * 0.003).exp(),
                    (f32::from(local.x), f32::from(local.y)),
                );
                cx.notify();
            }))
            .child(
                canvas(
                    move |b, _, cx| {
                        let changed = bounds.get().size != b.size;
                        bounds.set(b);
                        if changed {
                            let _ = view.update(cx, |_, cx| cx.notify());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        let mut y = 32.;
        let z = self.camera.zoom;
        let byte_scale = 800. / self.axis_span as f64;
        for region in &self.regions {
            area = area.child(
                div()
                    .absolute()
                    .left(px(self.camera.x + 20. * z))
                    .top(px(self.camera.y + y * z))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(memory_color(*region)))
                    .child(memory_label(*region)),
            );
            y += 40.;
            for (i, range) in self
                .ranges
                .iter()
                .enumerate()
                .filter(|(_, r)| r.space() == *region)
            {
                let relative = (i128::from(range.offset) - i128::from(self.axis_start)) as f64;
                let x = self.camera.x + (180. + (relative * byte_scale) as f32) * z;
                let top = self.camera.y + y * z;
                let width = ((range.length.unwrap_or(1) as f64 * byte_scale) as f32 * z).max(6.);
                let viewport = self.viewport.get();
                if top + (28. * z).max(18.) >= 0.
                    && top < f32::from(viewport.size.height)
                    && x + width >= 0.
                    && x < f32::from(viewport.size.width)
                {
                    area = area.child(
                        div()
                            .id(("memory-range", i))
                            .debug_selector(move || format!("memory-range-{i}"))
                            .absolute()
                            .left(px(x))
                            .top(px(top))
                            .w(px(width))
                            .h(px((24. * z).max(if self.selected == Some(i) {
                                14.
                            } else {
                                2.
                            })))
                            .rounded_sm()
                            .bg(rgb(memory_color(range.space())))
                            .border_2()
                            .border_color(rgb(if self.selected == Some(i) {
                                TEXT
                            } else {
                                memory_color(range.space())
                            }))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.moved {
                                    return;
                                }
                                this.select_range(i, cx);
                            })),
                    );
                    if z >= 0.5 || self.selected == Some(i) {
                        area = area.child(
                            div()
                                .debug_selector(move || format!("memory-label-{i}"))
                                .absolute()
                                .left(px(8.))
                                .top(px(top))
                                .w(px(120.))
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(format!("{} · 0x{:x}", range.label, range.offset)),
                        );
                    }
                }
                y += 32.;
            }
            y += 32.;
        }
        if self.ranges.is_empty() {
            area = area.child(div().p_4().child(if arena_view {
                "No planned tensor allocations are recorded for this method."
            } else {
                "No memory spans are recorded in this stream."
            }));
        }
        let mut legend = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .px_2()
            .pb_2()
            .text_size(px(11.));
        for space in &self.regions {
            legend = legend.child(
                div()
                    .text_color(rgb(memory_color(*space)))
                    .child(memory_label(*space)),
            );
        }
        let selected = self.selected.map(|i| {
            let r = &self.ranges[i];
            format!(
                "Selected: {} · {} · offset 0x{:x} · {}",
                memory_label(r.space()),
                r.label,
                r.offset,
                r.length
                    .map(|n| format!("{n} bytes"))
                    .unwrap_or_else(|| "length not configured".into())
            )
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .p_2()
                    .child(button("region-zoom-out", "−").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.zoom(this.camera.zoom / 1.25);
                            cx.notify();
                        },
                    )))
                    .child(
                        button("region-zoom-in", "+").on_click(cx.listener(|this, _, _, cx| {
                            this.zoom(this.camera.zoom * 1.25);
                            cx.notify();
                        })),
                    )
                    .when(arena_view, |toolbar| toolbar.child(button("arena-fit", "Fit arena").on_click(cx.listener(|this, _, _, cx| {
                        this.fit_arena(); cx.notify();
                    }))))
                    .child(button("region-fit", "Fit map").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.fit();
                            cx.notify();
                        },
                    )))
                    .child(button("region-focus", "Fit selection").on_click(cx.listener(|this, _, _, cx| {
                        this.focus_selected();
                        cx.notify();
                    })))
                    .child(button("region-actual", "100%").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.zoom(1.);
                            cx.notify();
                        },
                    ))),
            )
            .child(legend)
            .when_some(selected, |root, selected| root.child(div().px_2().pb_2().text_size(px(12.)).child(selected)))
            .child(area)
            .child(
                div()
                    .p_2()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "Offset axis: 0x{:x} … 0x{:x} bytes · Colors identify {} · White outline: selected · Drag to pan",
                        self.axis_start, self.axis_start.saturating_add(self.axis_span), if arena_view { "ExecuTorch arenas" } else { "NPU regions" }
                    )),
            )
    }
}
