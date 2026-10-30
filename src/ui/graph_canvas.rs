use super::*;

struct GraphFrame {
    graph: Graph,
    camera: Camera,
    nodes: AnyElement,
}
impl Explorer {
    fn prepare_canvas(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let changed = self.viewport.get().size != bounds.size;
        self.viewport.set(bounds);
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return;
        }
        let before = self.camera;
        let pending = self.fit_pending;
        if self.fit_pending || (self.auto_fit && changed) {
            self.camera.fit(
                &self.graph,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
            self.fit_pending = false;
        }
        if self.sidebar_visible && (changed || self.reveal_selection) {
            if let Some(node) = self
                .selected_graph_node
                .and_then(|i| self.graph.nodes.get(i))
            {
                self.camera.reveal(
                    node,
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                );
            }
        }
        self.reveal_selection = false;
        if pending
            || (before.x, before.y, before.zoom) != (self.camera.x, self.camera.y, self.camera.zoom)
        {
            // Update the toolbar's zoom label. The graph already uses this camera
            // in this frame; there is no second, differently scaled graph paint.
            cx.notify();
        }
    }
    pub(super) fn canvas(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        div()
            .id("graph-canvas")
            .debug_selector(|| "graph-canvas".into())
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(0x0e1520))
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
                        let delta = e.position - last;
                        if f32::from(delta.x).abs() + f32::from(delta.y).abs() > 1. {
                            this.moved = true;
                            this.auto_fit = false;
                            this.camera.x += f32::from(delta.x);
                            this.camera.y += f32::from(delta.y);
                            cx.notify();
                        }
                        this.drag = Some(e.position);
                    }
                } else {
                    this.drag = None;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                if this.moved {
                    return;
                }
                let Some(position) = e.mouse_position() else {
                    return;
                };
                let local = position - this.viewport.get().origin;
                let x = (f32::from(local.x) - this.camera.x) / this.camera.zoom;
                let y = (f32::from(local.y) - this.camera.y) / this.camera.zoom;
                if !this
                    .graph
                    .nodes
                    .iter()
                    .any(|n| x >= n.x && x <= n.x + n.w && y >= n.y && y <= n.y + n.h)
                {
                    this.clear_selection(cx);
                }
            }))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                this.auto_fit = false;
                let local = e.position - this.viewport.get().origin;
                let delta = e.delta.pixel_delta(px(24.));
                this.camera.zoom_at(
                    this.camera.zoom * (f32::from(delta.y) * 0.003).exp(),
                    (f32::from(local.x), f32::from(local.y)),
                );
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let mut frame = view
                            .update(cx, |this, cx| {
                                this.prepare_canvas(bounds, cx);
                                GraphFrame {
                                    graph: this.graph.clone(),
                                    camera: this.camera,
                                    nodes: this.graph_nodes(cx).into_any_element(),
                                }
                            })
                            .ok();
                        if let Some(frame) = &mut frame {
                            frame.nodes.prepaint_as_root(
                                bounds.origin,
                                size(
                                    AvailableSpace::Definite(bounds.size.width),
                                    AvailableSpace::Definite(bounds.size.height),
                                ),
                                window,
                                cx,
                            );
                        }
                        frame
                    },
                    move |bounds, frame, window, cx| {
                        let Some(mut frame) = frame else {
                            return;
                        };
                        paint_edges(&frame.graph, frame.camera, bounds, window);
                        frame.nodes.paint(window, cx);
                    },
                )
                .absolute()
                .size_full(),
            )
    }
    fn graph_nodes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut layer = div().id("graph-nodes").relative().size_full();
        for (i, node) in self.graph.nodes.iter().enumerate() {
            let selection = node.selection;
            let selected = self.selected_graph_node == Some(i);
            let z = self.camera.zoom;
            layer = layer.child(
                div()
                    .id(("node", i))
                    .debug_selector(move || format!("graph-node-{i}"))
                    .absolute()
                    .left(px(self.camera.x + node.x * z))
                    .top(px(self.camera.y + node.y * z))
                    .w(px(node.w * z))
                    .h(px(node.h * z))
                    .p(px(12. * z))
                    .rounded(px(8. * z))
                    .border_1()
                    .border_color(rgb(if selected { ACCENT } else { BORDER }))
                    .bg(rgb(if node.delegate {
                        0x1d3941
                    } else if node.title.starts_with("INPUT") {
                        0x193b35
                    } else if node.title.starts_with("OUTPUT") {
                        0x203a52
                    } else {
                        PANEL
                    }))
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if !this.moved {
                            this.select(selection, cx);
                            this.selected_graph_node = Some(i);
                            this.last_node_click =
                                event.mouse_position().map(|position| (selection, position));
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(16. * z))
                            .text_color(rgb(TEXT))
                            .child(node.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13. * z))
                            .text_color(rgb(MUTED))
                            .child(node.subtitle.clone()),
                    )
                    .children(node.properties.iter().map(|(name, value)| {
                        div()
                            .flex()
                            .gap(px(6. * z))
                            .text_size(px(13. * z))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(name.clone()),
                            )
                            .child(div().text_color(rgb(MUTED)).child(value.clone()))
                    })),
            );
        }
        layer
    }
}

fn paint_edges(graph: &Graph, camera: Camera, bounds: Bounds<Pixels>, window: &mut Window) {
    let screen = |x: f32, y: f32| {
        point(
            bounds.origin.x + px(camera.x + x * camera.zoom),
            bounds.origin.y + px(camera.y + y * camera.zoom),
        )
    };
    for edge in &graph.edges {
        let a = &graph.nodes[edge.from];
        let b = &graph.nodes[edge.to];
        let (start, end) = if edge.execution || (a.x - b.x).abs() < 20. {
            (
                screen(a.x + a.w / 2., a.y + a.h),
                screen(b.x + b.w / 2., b.y),
            )
        } else {
            (
                screen(a.x + a.w, a.y + a.h / 2.),
                screen(b.x, b.y + b.h / 2.),
            )
        };
        let mut path = PathBuilder::stroke(px(if edge.execution { 2. } else { 1. }));
        path.move_to(start);
        path.line_to(end);
        if edge.directed {
            let d = end - start;
            let length = (f32::from(d.x).powi(2) + f32::from(d.y).powi(2))
                .sqrt()
                .max(1.);
            let (dx, dy) = (f32::from(d.x) / length, f32::from(d.y) / length);
            path.move_to(end + point(px(-dx * 9. - dy * 4.), px(-dy * 9. + dx * 4.)));
            path.line_to(end);
            path.line_to(end + point(px(-dx * 9. + dy * 4.), px(-dy * 9. - dx * 4.)));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(if edge.execution { ACCENT } else { 0x3a526c }));
        }
    }
}
