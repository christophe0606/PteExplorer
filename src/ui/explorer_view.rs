use super::*;
impl Render for Explorer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width);
        let details_width = if self.stream.is_some() {
            (width * 0.62).clamp(340., 850.)
        } else {
            (width * 0.38).clamp(280., 440.)
        };
        let method_label = self
            .program
            .as_ref()
            .and_then(|p| p.methods.get(self.method))
            .map(|m| format!("Method: {} ▾", m.name))
            .unwrap_or_else(|| "Method ▾".into());
        let mut dropdown = div().relative().flex_shrink_0().child(
            button("method-dropdown", method_label).on_click(cx.listener(|this, _, _, cx| {
                this.method_menu = !this.method_menu;
                cx.notify();
            })),
        );
        if self.method_menu {
            let mut menu = div()
                .id("method-menu")
                .debug_selector(|| "method-menu".into())
                .absolute()
                .top(px(44.))
                .left(px(0.))
                .w(px(280.))
                .max_h(px(320.))
                .overflow_y_scroll()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded_md()
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.method_menu = false;
                    cx.notify();
                }));
            if let Some(p) = &self.program {
                for (i, m) in p.methods.iter().enumerate() {
                    menu = menu.child(
                        div()
                            .id(("method-option", i))
                            .debug_selector(move || format!("method-option-{i}"))
                            .p_3()
                            .bg(rgb(if i == self.method { 0x2b4e61 } else { PANEL }))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x344d64)))
                            .child(format!(
                                "{} {}",
                                if i == self.method { "✓" } else { "  " },
                                m.name
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| this.set_method(i, cx))),
                    );
                }
            } else {
                menu = menu.child(div().p_3().child("Open a model to select a method"));
            }
            dropdown = dropdown.child(deferred(menu).with_priority(10));
        }
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .bg(rgb(PANEL))
            .child(
                button("arenas", "Arenas")
                    .on_click(cx.listener(|this, _, _, cx| this.open_arenas(cx))),
            )
            .child(
                button("zoom-out", "−")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom(this.camera.zoom / 1.2, cx))),
            )
            .child(
                div()
                    .w(px(52.))
                    .child(format!("{:.0}%", self.camera.zoom * 100.)),
            )
            .child(
                button("zoom-in", "+")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom(this.camera.zoom * 1.2, cx))),
            )
            .child(button("fit", "Fit").on_click(cx.listener(|this, _, _, cx| {
                this.auto_fit = true;
                let b = this.viewport.get();
                this.camera.fit(
                    &this.graph,
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                );
                cx.notify();
            })))
            .child(
                button("actual-size", "100%")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom(1., cx))),
            );
        let main = div().flex().flex_col().flex_1().min_w_0().h_full().child(toolbar).child(self.canvas(cx))
            .child(div().p_2().text_size(px(11.)).text_color(rgb(MUTED)).child("Operators connect directly · Tensors and arena links: operator details · Drag to pan, wheel to zoom · Double-click a node: separate details window"));
        let right = self.sidebar_visible.then(|| {
            let right = if let Some(stream) = &self.stream {
                div().w(px(details_width)).h_full().child(stream.clone())
            } else {
                if self.detail_panel.is_none() {
                    let (title, sections) =
                        explorer_document(self.program.as_deref(), self.method, self.selection);
                    self.detail_panel = Some(cx.new(|_| {
                        let panel = DetailPanel::new(title, sections);
                        if let Some(m) = self
                            .program
                            .as_ref()
                            .and_then(|p| p.methods.get(self.method))
                        {
                            panel.with_method(m)
                        } else {
                            panel
                        }
                    }));
                }
                div()
                    .w(px(details_width))
                    .h_full()
                    .child(self.detail_panel.as_ref().unwrap().clone())
            }
            .debug_selector(|| "right-pane".into())
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden();
            right.into_any_element()
        });
        let mut root = div()
            .size_full()
            .capture_any_mouse_down(
                cx.listener(|this, event, _, cx| this.capture_node_double_click(event, cx)),
            )
            .flex()
            .flex_col()
            .font_family(system_font())
            .text_size(px(14.))
            .text_color(rgb(TEXT))
            .bg(rgb(BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .child("PTE Explorer"),
                    )
                    .child(
                        button("open", "Open .pte…")
                            .on_click(cx.listener(|this, _, _, cx| this.open(cx))),
                    )
                    .child(dropdown)
                    .child(
                        button("metadata", "File & method info").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.selection = None;
                                this.selected_graph_node = None;
                                this.sidebar_visible = true;
                                this.reveal_selection = false;
                                this.stream = None;
                                this.detail_panel = None;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        button(
                            "toggle-sidebar",
                            if self.sidebar_visible {
                                "Hide sidebar"
                            } else {
                                "Show sidebar"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .pb_2()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(if self.loading {
                        "Loading…".into()
                    } else {
                        self.program
                            .as_ref()
                            .map(|p| p.path.clone())
                            .unwrap_or_else(|| "ExecuTorch · Ethos-U55 / U65 / U85".into())
                    }),
            );
        if let Some(error) = &self.error {
            root = root.child(div().p_3().bg(rgb(0x5c2830)).child(error.clone()));
        }
        root.child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(main)
                .when_some(right, |body, right| body.child(right)),
        )
    }
}
