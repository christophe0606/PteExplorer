use super::*;

pub(super) struct StreamView {
    program: Arc<Program>,
    method: usize,
    delegate: usize,
    pub(super) selected: Option<usize>,
    pub(super) operations_only: bool,
    pub(super) memory_view: bool,
    pub(super) scroll: UniformListScrollHandle,
    rail: ScrollRail,
    pub(super) detail: Option<Entity<DetailPanel>>,
    pub(super) region_map: Option<Entity<RegionMap>>,
    memory_ranges: Arc<Vec<pte_explorer::vela::MemoryRange>>,
}
impl StreamView {
    pub(super) fn new(program: Arc<Program>, method: usize, delegate: usize) -> Self {
        let scroll = UniformListScrollHandle::new();
        let rail = ScrollRail::new(scroll.0.borrow().base_handle.clone());
        let memory_ranges = Arc::new(
            program.methods[method].delegates[delegate]
                .vela
                .as_ref()
                .unwrap()
                .memory_ranges(),
        );
        Self {
            program,
            method,
            delegate,
            selected: None,
            operations_only: false,
            memory_view: false,
            scroll,
            rail,
            detail: None,
            region_map: None,
            memory_ranges,
        }
    }
    fn vela(&self) -> &pte_explorer::vela::Vela {
        self.program.methods[self.method].delegates[self.delegate]
            .vela
            .as_ref()
            .unwrap()
    }
    fn overview(&self) -> Vec<Section> {
        let d = &self.program.methods[self.method].delegates[self.delegate];
        let v = self.vela();
        let mut overview = Section::new("Delegate")
            .field("Backend", &d.id)
            .field("Commands", v.instructions.len())
            .field("Size", format!("{} bytes", d.size))
            .field(
                "Scratch arena",
                v.scratch_size
                    .map(|s| format!("{s} bytes"))
                    .unwrap_or_else(|| "Not recorded".into()),
            );
        if let Some(h) = &v.config {
            overview = overview
                .field("Architecture", &h.architecture)
                .field("MACs / cycle", h.macs_per_cycle)
                .field("Stream version", h.stream_version);
        }
        let mut sections = vec![overview];
        let mut config = Section::new("Vela configuration");
        for spec in &d.specs {
            config = config.field(&spec.key, spec.text.as_deref().unwrap_or(&spec.hex));
        }
        config = config.field("Availability", "Only recorded compile options are shown. An INI path does not embed the file's contents.");
        sections.push(config);
        let method = &self.program.methods[self.method];
        for (i, op) in method
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, op)| op.delegate == Some(self.delegate))
        {
            for (&arg, role) in op.args.iter().zip(argument_roles(method, i)) {
                sections.push(value_section(
                    &method.values[arg as usize],
                    arg,
                    match role {
                        ArgumentRole::Input => "Delegate input",
                        ArgumentRole::Output => "Delegate output",
                        ArgumentRole::Unknown => "Unclassified argument",
                    },
                ));
            }
        }
        sections.push(pte_explorer::vela::addressing_help());
        for (region, uses) in v.region_uses() {
            sections.push(v.region_section(region).field(
                "Observed command uses",
                uses.into_iter().collect::<Vec<_>>().join(", "),
            ));
        }
        let mut blocks = Section::new("Container blocks");
        for b in &v.blocks {
            blocks = blocks.field(
                &b.name,
                format!(
                    "{} bytes{}",
                    b.size,
                    b.external_key
                        .as_ref()
                        .map(|k| format!(" · named data: {k}"))
                        .unwrap_or_default()
                ),
            );
        }
        sections.push(blocks);
        if !v.warnings.is_empty() {
            let mut s = Section::new("Decode warnings");
            for (i, w) in v.warnings.iter().enumerate() {
                s = s.field(format!("Warning {}", i + 1), w);
            }
            sections.push(s);
        }
        sections
    }
    fn select(&mut self, i: usize, cx: &mut Context<Self>) {
        self.selected = Some(i);
        self.detail = None;
        cx.notify();
    }
    fn set_filter(&mut self, operations: bool, cx: &mut Context<Self>) {
        self.operations_only = operations;
        if operations
            && self
                .selected
                .is_some_and(|i| !self.vela().instructions[i].name.starts_with("NPU_OP_"))
        {
            self.selected = None;
            self.detail = None;
        }
        self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
        cx.notify();
    }
}
impl Render for StreamView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let indices: Arc<Vec<usize>> = Arc::new(
            self.vela()
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, c)| !self.operations_only || c.name.starts_with("NPU_OP_"))
                .map(|(i, _)| i)
                .collect(),
        );
        let count = indices.len();
        let total = self.vela().instructions.len();
        let bar = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .p_2()
            .bg(rgb(PANEL))
            .items_center()
            .child(
                button(
                    "stream-tab",
                    if !self.memory_view {
                        "✓ Command stream"
                    } else {
                        "Command stream"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.memory_view = false;
                    cx.notify();
                })),
            )
            .child(
                button(
                    "memory-tab",
                    if self.memory_view {
                        "✓ Region map"
                    } else {
                        "Region map"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.memory_view = true;
                    cx.notify();
                })),
            )
            .child(
                button("regions", "Show delegate overview").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.selected = None;
                        this.detail = None;
                        this.memory_view = false;
                        cx.notify();
                    },
                )),
            );
        if self.region_map.is_none() && self.memory_view {
            let ranges = self.memory_ranges.clone();
            let map = cx.new(|cx| RegionMap::new(ranges, cx));
            cx.observe(&map, |_, _, cx| cx.notify()).detach();
            self.region_map = Some(map);
        }
        if self.detail.is_none() {
            let title = self
                .selected
                .map(|i| self.vela().instructions[i].name.clone())
                .unwrap_or_else(|| "Delegate overview".into());
            let sections = self
                .selected
                .map(|i| self.vela().instruction_sections(i))
                .unwrap_or_else(|| self.overview());
            let ranges = self.memory_ranges.clone();
            self.detail = Some(cx.new(|_| {
                DetailPanel::new(title, sections)
                    .with_memory_ranges(ranges)
                    .with_method(&self.program.methods[self.method])
            }));
        }
        let detail = if self.memory_view {
            self.region_map.as_ref().unwrap().read(cx).detail.clone()
        } else {
            self.detail.as_ref().unwrap().clone()
        };
        let copy = detail.read(cx).copy_text();
        let left: AnyElement = if self.memory_view {
            self.region_map.as_ref().unwrap().clone().into_any_element()
        } else {
            let list = uniform_list(
                "commands",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|row| {
                            let i = indices[row];
                            let c = &this.vela().instructions[i];
                            div()
                                .id(("command", i))
                                .debug_selector(move || format!("command-{i}"))
                                .h(px(52.))
                                .px_3()
                                .py_1()
                                .bg(rgb(if this.selected == Some(i) {
                                    0x284b58
                                } else if c.name.starts_with("NPU_OP_") {
                                    0x203244
                                } else {
                                    BG
                                }))
                                .border_b_1()
                                .border_color(rgb(0x223044))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx)))
                                .child(
                                    div()
                                        .text_ellipsis()
                                        .text_size(px(11.))
                                        .text_color(rgb(MUTED))
                                        .child(format!(
                                            "{i:04}  +0x{:06x}  0x{:08x}",
                                            c.offset, c.raw
                                        )),
                                )
                                .child(
                                    div()
                                        .text_ellipsis()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(c.name.clone()),
                                )
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(self.scroll.clone())
            .flex_1()
            .min_w_0()
            .h_full()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()));
            div()
                .flex()
                .flex_col()
                .size_full()
                .min_w_0()
                .child(
                    div()
                        .p_2()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            button(
                                "filter-all",
                                if self.operations_only {
                                    "All commands"
                                } else {
                                    "✓ All commands"
                                },
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.set_filter(false, cx))),
                        )
                        .child(
                            button(
                                "filter-operations",
                                if self.operations_only {
                                    "✓ Operations only"
                                } else {
                                    "Operations only"
                                },
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.set_filter(true, cx))),
                        ),
                )
                .child(
                    div()
                        .px_2()
                        .pb_2()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(format!("Showing {count} of {total} commands")),
                )
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(list)
                        .child(self.rail.render("command-scrollbar", cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_2()
                        .p_2()
                        .child(button("command-first", "First").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                                cx.notify();
                            },
                        )))
                        .child(button("command-last", "Last").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.scroll.scroll_to_item_strict(
                                    count.saturating_sub(1),
                                    ScrollStrategy::Bottom,
                                );
                                cx.notify();
                            },
                        ))),
                )
                .into_any_element()
        };
        div()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family(system_font())
            .text_size(px(14.))
            .text_color(rgb(TEXT))
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(bar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().min_w_0().h_full().child(left))
                    .child(
                        div()
                            .debug_selector(|| "stream-details-pane".into())
                            .w(relative(0.5))
                            .flex_shrink_0()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(button("copy-details", "Copy details").on_click(
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                                },
                            ))
                            .child(div().flex_1().min_h_0().child(detail)),
                    ),
            )
    }
}
