use gpui::{prelude::*, *};
use pte_explorer::{
    graph::{Camera, Graph, Selection},
    pte::Program,
};
use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};
mod explorer_view;
mod graph_canvas;
mod memory_window;
mod panels;
mod stream;
use memory_window::MemoryWindow;
use panels::{DetailPanel, RegionMap, ScrollRail, explorer_document, value_section};
use pte_explorer::{
    graph::{ArgumentRole, argument_roles},
    vela::InspectionSection as Section,
};
use stream::StreamView;

const BG: u32 = 0x111822;
const PANEL: u32 = 0x192332;
const BORDER: u32 = 0x304058;
const TEXT: u32 = 0xe6edf7;
const MUTED: u32 = 0x9dafc5;
const ACCENT: u32 = 0x60d4c5;
fn system_font() -> &'static str {
    if cfg!(target_os = "windows") {
        "Segoe UI"
    } else if cfg!(target_os = "macos") {
        "Helvetica Neue"
    } else {
        "DejaVu Sans"
    }
}
fn button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px_3()
        .py_2()
        .flex_shrink_0()
        .rounded_md()
        .bg(rgb(0x293a50))
        .text_color(rgb(TEXT))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3b526f)))
        .child(label.into())
}
fn fit_window_bounds(work_area: Bounds<Pixels>, preferred: Size<Pixels>) -> Bounds<Pixels> {
    // Bounds describe the content area. Leave room for the native title bar,
    // resize borders and shadows as well as a little space around the window.
    let margin = px(32.)
        .min(work_area.size.width / 4.)
        .min(work_area.size.height / 4.);
    let available = work_area.size - size(margin * 2., margin * 2.);
    Bounds::centered_at(work_area.center(), preferred.min(&available))
}

#[cfg(target_os = "windows")]
fn windows_work_area() -> Option<Bounds<Pixels>> {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint},
        UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
    };
    // The primary monitor contains (0, 0), matching GPUI's primary display.
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96, 96);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.ok()?;
    let scale = dpi_x as f32 / 96.;
    let work = info.rcWork;
    // GPUI 0.2.2 passes these bounds to SetWindowPlacement. For normal Windows
    // windows that API uses workspace coordinates, whose origin excludes any
    // top/left taskbar: https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-windowplacement
    Some(Bounds::new(
        point(px(0.), px(0.)),
        size(
            px((work.right - work.left) as f32 / scale),
            px((work.bottom - work.top) as f32 / scale),
        ),
    ))
}

fn window_options(title: String, w: f32, h: f32, cx: &App) -> WindowOptions {
    let display = cx.primary_display();
    let work_area = display.as_ref().map(|display| {
        #[cfg(target_os = "windows")]
        if let Some(area) = windows_work_area() {
            return area;
        }
        display.bounds()
    });
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        window_bounds: work_area
            .map(|area| WindowBounds::Windowed(fit_window_bounds(area, size(px(w), px(h))))),
        display_id: display.map(|display| display.id()),
        ..Default::default()
    }
}
pub fn run(path: Option<PathBuf>) {
    Application::new().run(move |cx| {
        cx.open_window(
            window_options("PTE Explorer".into(), 1500., 940., cx),
            |_, cx| {
                cx.new(|cx| {
                    let mut view = Explorer::empty();
                    if let Some(path) = path {
                        view.load(path, cx);
                    }
                    view
                })
            },
        )
        .expect("open GPUI window");
        cx.activate(true);
    });
}
struct Explorer {
    program: Option<Arc<Program>>,
    method: usize,
    selection: Option<Selection>,
    selected_graph_node: Option<usize>,
    sidebar_visible: bool,
    reveal_selection: bool,
    graph: Graph,
    camera: Camera,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    drag: Option<Point<Pixels>>,
    moved: bool,
    stream: Option<Entity<StreamView>>,
    error: Option<String>,
    loading: bool,
    load_generation: u64,
    method_menu: bool,
    detail_panel: Option<Entity<DetailPanel>>,
    fit_pending: bool,
    auto_fit: bool,
    last_node_click: Option<(Selection, Point<Pixels>)>,
}
impl Explorer {
    fn empty() -> Self {
        Self {
            program: None,
            method: 0,
            selection: None,
            selected_graph_node: None,
            sidebar_visible: true,
            reveal_selection: false,
            graph: Graph::default(),
            camera: Camera::default(),
            viewport: Rc::new(Cell::new(Bounds::default())),
            drag: None,
            moved: false,
            stream: None,
            error: None,
            loading: false,
            load_generation: 0,
            method_menu: false,
            detail_panel: None,
            fit_pending: false,
            auto_fit: false,
            last_node_click: None,
        }
    }
    fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loading = true;
        self.error = None;
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { Program::open(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.load_generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(p) => {
                        this.program = Some(Arc::new(p));
                        this.set_method(0, cx);
                    }
                    Err(e) => this.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn open(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open ExecuTorch .pte".into()),
        });
        cx.spawn(async move |this, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| this.load(path, cx));
                }
            }
            Ok(Ok(None)) => {}
            error => {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("File picker: {error:?}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn set_method(&mut self, index: usize, cx: &mut Context<Self>) {
        self.method = index;
        self.selection = None;
        self.selected_graph_node = None;
        self.reveal_selection = false;
        self.stream = None;
        self.method_menu = false;
        self.detail_panel = None;
        self.camera = Camera::default();
        self.last_node_click = None;
        self.fit_pending = true;
        self.auto_fit = true;
        self.graph = self
            .program
            .as_ref()
            .and_then(|p| p.methods.get(index))
            .map(Graph::method)
            .unwrap_or_default();
        cx.notify();
    }
    fn select(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.selection = Some(selection);
        self.selected_graph_node = self
            .graph
            .nodes
            .iter()
            .position(|n| n.selection == selection);
        self.sidebar_visible = true;
        self.reveal_selection = true;
        if !self.fit_pending {
            self.auto_fit = false;
        }
        self.stream = None;
        self.detail_panel = None;
        if let Selection::Operator(i) = selection {
            if let Some(p) = &self.program {
                if let Some(d) = p.methods[self.method].instructions[i].delegate {
                    if p.methods[self.method].delegates[d].vela.is_some() {
                        self.stream = Some(cx.new(|_| StreamView::new(p.clone(), self.method, d)));
                    }
                }
            }
        }
        cx.notify();
    }
    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        self.selected_graph_node = None;
        self.sidebar_visible = false;
        self.reveal_selection = false;
        self.stream = None;
        self.detail_panel = None;
        self.last_node_click = None;
        cx.notify();
    }
    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        self.reveal_selection = self.sidebar_visible && self.selection.is_some();
        cx.notify();
    }
    fn open_node_details(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let Some(program) = self.program.clone() else {
            return;
        };
        if let Selection::Operator(i) = selection {
            if let Some(delegate) = program.methods[self.method].instructions[i].delegate {
                if program.methods[self.method].delegates[delegate]
                    .vela
                    .is_some()
                {
                    let method = self.method;
                    let options = window_options(
                        format!(
                            "Vela · {} · delegate {delegate}",
                            program.methods[method].name
                        ),
                        1450.,
                        900.,
                        cx,
                    );
                    if let Err(error) = cx.open_window(options, move |_, cx| {
                        cx.new(|_| StreamView::new(program, method, delegate))
                    }) {
                        self.error = Some(format!("Cannot open node details: {error:#}"));
                    }
                    cx.notify();
                    return;
                }
            }
        }
        let (title, sections) = explorer_document(Some(&program), self.method, Some(selection));
        let method = self.method;
        let options = window_options(
            format!("{title} · {}", program.methods[self.method].name),
            620.,
            800.,
            cx,
        );
        if let Err(error) = cx.open_window(options, move |_, cx| {
            cx.new(|_| DetailPanel::new(title, sections).with_method(&program.methods[method]))
        }) {
            self.error = Some(format!("Cannot open node details: {error:#}"));
        }
        cx.notify();
    }
    fn open_arenas(&mut self, cx: &mut Context<Self>) {
        let Some(method) = self
            .program
            .as_ref()
            .and_then(|p| p.methods.get(self.method))
        else {
            return;
        };
        let ranges = Arc::new(pte_explorer::arena::ranges(method));
        let options = window_options(
            format!("ExecuTorch arenas · {}", method.name),
            1250.,
            850.,
            cx,
        );
        if let Err(error) = cx.open_window(options, move |_, cx| {
            cx.new(|cx| MemoryWindow::overview(ranges, cx))
        }) {
            self.error = Some(format!("Cannot open arena view: {error:#}"));
            cx.notify();
        }
    }
    fn capture_node_double_click(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        // Selecting an Ethos node resizes the canvas. Remember the first hit
        // so the second click still targets that node if it has moved.
        let previous = self.last_node_click.take();
        if event.button == MouseButton::Left && event.click_count == 2 {
            if let Some((selection, position)) = previous {
                let delta = event.position - position;
                if f32::from(delta.x).abs() <= 8. && f32::from(delta.y).abs() <= 8. {
                    self.open_node_details(selection, cx);
                    cx.stop_propagation();
                }
            }
        }
    }
    fn zoom(&mut self, new: f32, cx: &mut Context<Self>) {
        self.auto_fit = false;
        let bounds = self.viewport.get();
        self.camera.zoom_at(
            new,
            (
                f32::from(bounds.size.width) / 2.,
                f32::from(bounds.size.height) / 2.,
            ),
        );
        cx.notify();
    }
}
#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use pte_explorer::pte::{Instruction, Method, Value};
    mod mixed {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/mixed.rs"
        ));
    }
    fn assert_graph_node_visible(
        view: &Entity<Explorer>,
        cx: &mut VisualTestContext,
        selection: Selection,
    ) {
        let i = view.read_with(cx, |v, _| {
            assert_eq!(v.selection, Some(selection));
            v.selected_graph_node.unwrap()
        });
        let id: &'static str = Box::leak(format!("graph-node-{i}").into_boxed_str());
        let node = cx.debug_bounds(id).unwrap();
        let canvas = cx.debug_bounds("graph-canvas").unwrap();
        assert!(
            node.left() >= canvas.left()
                && node.right() <= canvas.right()
                && node.top() >= canvas.top()
                && node.bottom() <= canvas.bottom(),
            "Selected node {node:?} must remain inside canvas {canvas:?}"
        );
    }
    #[gpui::test]
    fn first_graph_paint_uses_the_fitted_camera(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| Explorer::empty());
        cx.simulate_resize(size(px(1400.), px(900.)));
        cx.run_until_parked();
        view.update(cx, |v, cx| {
            let mut program = fixture();
            Arc::get_mut(&mut program).unwrap().methods[0] = mixed::method();
            v.program = Some(program);
            v.set_method(0, cx);
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let (camera, viewport, nodes) = view.read_with(cx, |v, _| {
            assert!(!v.fit_pending);
            assert!(v.graph.width * v.camera.zoom <= f32::from(v.viewport.get().size.width) - 39.);
            (v.camera, v.viewport.get(), v.graph.nodes.clone())
        });
        for (i, node) in nodes.iter().enumerate() {
            let id: &'static str = Box::leak(format!("graph-node-{i}").into_boxed_str());
            let painted = cx.debug_bounds(id).unwrap();
            assert!(
                (painted.left() - viewport.left() - px(camera.x + node.x * camera.zoom)).abs()
                    < px(1.),
                "First frame: {painted:?}; camera {camera:?}; viewport {viewport:?}; node {node:?}"
            );
            assert!(
                (painted.top() - viewport.top() - px(camera.y + node.y * camera.zoom)).abs()
                    < px(1.)
            );
            assert!((painted.size.width - px(node.w * camera.zoom)).abs() < px(1.));
        }
        cx.run_until_parked();
        let after = view.read_with(cx, |v, _| v.camera);
        assert_eq!(
            (camera.x, camera.y, camera.zoom),
            (after.x, after.y, after.zoom),
            "The first graph must not switch camera after painting"
        );
    }
    #[gpui::test]
    fn sidebar_toggle_blank_click_and_selected_node_visibility(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut v = Explorer::empty();
            v.program = Some(delegate_fixture());
            v.set_method(0, cx);
            v
        });
        cx.simulate_resize(size(px(1300.), px(850.)));
        cx.run_until_parked();
        let initial_width = view.read_with(cx, |v, _| v.viewport.get().size.width);
        click(cx, "toggle-sidebar");
        cx.run_until_parked();
        assert!(view.read_with(cx, |v, _| !v.sidebar_visible));
        assert!(
            (view.read_with(cx, |v, _| v.viewport.get().size.width) - px(1300.)).abs() < px(1.)
        );
        assert!(view.read_with(cx, |v, _| v.viewport.get().size.width) > initial_width);
        click(cx, "actual-size");
        view.update(cx, |v, cx| {
            let node = v
                .graph
                .nodes
                .iter()
                .find(|n| n.selection == Selection::Operator(0))
                .unwrap();
            v.camera.x = f32::from(v.viewport.get().size.width) - node.x - node.w - 32.;
            v.camera.y = 48. - node.y;
            v.auto_fit = false;
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let i = view.read_with(cx, |v, _| {
            v.graph
                .nodes
                .iter()
                .position(|n| n.selection == Selection::Operator(0))
                .unwrap()
        });
        let id: &'static str = Box::leak(format!("graph-node-{i}").into_boxed_str());
        let node = cx.debug_bounds(id).unwrap();
        assert!(
            node.left() > px(700.),
            "Reproduce a node beyond the future narrowed canvas"
        );
        cx.simulate_click(node.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(view.read_with(cx, |v, _| v.sidebar_visible
            && v.selection == Some(Selection::Operator(0))
            && v.stream.is_some()));
        assert_eq!(
            view.read_with(cx, |v, _| v.camera.zoom),
            1.,
            "Opening details should pan without unnecessary zoom changes"
        );
        assert_graph_node_visible(&view, cx, Selection::Operator(0));
        click(cx, "toggle-sidebar");
        assert!(view.read_with(cx, |v, _| !v.sidebar_visible && v.selection.is_some()));
        click(cx, "toggle-sidebar");
        cx.run_until_parked();
        assert_graph_node_visible(&view, cx, Selection::Operator(0));
        for (width, height) in [(700., 500.), (900., 800.), (1300., 850.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            assert_graph_node_visible(&view, cx, Selection::Operator(0));
        }
        let blank = cx.debug_bounds("graph-canvas").unwrap().bottom_left() + point(px(8.), px(-8.));
        cx.simulate_mouse_down(blank, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            blank + point(px(20.), px(-20.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            blank + point(px(20.), px(-20.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert!(
            view.read_with(cx, |v, _| v.sidebar_visible && v.selection.is_some()),
            "Panning must not clear selection"
        );
        cx.simulate_click(blank, Modifiers::none());
        cx.run_until_parked();
        assert!(view.read_with(cx, |v, _| v.selection.is_none()
            && !v.sidebar_visible
            && v.stream.is_none()
            && v.last_node_click.is_none()));
        assert!(
            (view.read_with(cx, |v, _| v.viewport.get().size.width) - px(1300.)).abs() < px(1.)
        );
        click(cx, "metadata");
        assert!(view.read_with(cx, |v, _| v.sidebar_visible && v.selection.is_none()));
        // An in-place method may expose the same value ID at both boundaries.
        // Reveal the clicked boundary, rather than the first node with that ID.
        view.update(cx, |v, cx| {
            let mut program = fixture();
            let mut method = mixed::method();
            method.instructions.truncate(1);
            method.instructions[0].name = "aten::relu_".into();
            method.instructions[0].args = vec![0, 0];
            method.outputs = vec![0];
            Arc::get_mut(&mut program).unwrap().methods[0] = method;
            v.program = Some(program);
            v.set_method(0, cx);
            v.sidebar_visible = false;
            v.fit_pending = false;
            v.auto_fit = false;
            let node = v.graph.nodes.last().unwrap();
            v.camera = Camera {
                zoom: 1.,
                x: 1300. - node.x - node.w - 32.,
                y: 48. - node.y,
            };
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let last = view.read_with(cx, |v, _| v.graph.nodes.len() - 1);
        let output_id: &'static str = Box::leak(format!("graph-node-{last}").into_boxed_str());
        let output = cx.debug_bounds(output_id).unwrap();
        cx.simulate_click(output.center(), Modifiers::none());
        assert_eq!(view.read_with(cx, |v, _| v.selected_graph_node), Some(last));
        assert_graph_node_visible(&view, cx, Selection::Value(0));
    }
    #[gpui::test]
    fn arena_link_opens_focused_tensor_map(cx: &mut TestAppContext) {
        let (panel, cx) = cx.add_window_view(|_, _| {
            let mut method = mixed::method();
            method.memory_sizes.push(0x1_0001_0000);
            method.values[3].tensor.as_mut().unwrap().allocation = Some((2, 0x1_0000_0000));
            DetailPanel::new(
                "Tensor output",
                vec![value_section(&method.values[3], 3, "Output")],
            )
            .with_method(&method)
        });
        cx.simulate_resize(size(px(620.), px(900.)));
        cx.run_until_parked();
        let field = panel.read_with(cx, |p, _| {
            let j = *p.sections[0].memory_links.keys().next().unwrap();
            assert!(p.sections[0].fields[j].1.contains("Arena 2"));
            assert!(p.copy_text().contains("byte offset 4294967296"));
            j
        });
        let id: &'static str = Box::leak(format!("field-0-{field}").into_boxed_str());
        let before = cx.windows().len();
        click(cx, id);
        cx.run_until_parked();
        assert_eq!(
            cx.windows().len(),
            before + 1,
            "One click opens the arena view"
        );
        let window = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<MemoryWindow>())
            .unwrap();
        let root = window.root(cx).unwrap();
        let map = root.read_with(cx, |w, _| w.map.clone());
        let mut map_cx = VisualTestContext::from_window(window.into(), cx);
        map_cx.run_until_parked();
        map_cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let selected = map.read_with(&map_cx, |m, _| {
            let selected = m.selected.unwrap();
            assert_eq!(
                m.ranges[selected].space(),
                pte_explorer::vela::MemorySpace::Arena(2)
            );
            assert_eq!(m.ranges[selected].label, "v3");
            assert_eq!(m.ranges[selected].length, Some(12288));
            assert_eq!(m.ranges.len(), 9); // two capacities and seven allocated tensors
            assert!(m.axis_span < 32768);
            selected
        });
        let bar_id: &'static str = Box::leak(format!("memory-range-{selected}").into_boxed_str());
        let canvas = map_cx.debug_bounds("region-canvas").unwrap();
        let bar = map_cx.debug_bounds(bar_id).unwrap();
        assert!(bar.left() >= canvas.left() && bar.right() <= canvas.right());
        assert!((bar.center().y - canvas.center().y).abs() < px(2.));
        click(&mut map_cx, "arena-fit");
        assert!(map.read_with(&map_cx, |m, _| m.axis_span) > 0x1_0000_0000);
        click(&mut map_cx, "region-focus");
        assert!(map.read_with(&map_cx, |m, _| m.axis_span) < 32768);
        click(&mut map_cx, "copy-details");
        let copied = map_cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copied.contains("ExecuTorch arena ID: 2") && copied.contains("v3"));
        assert!(!copied.contains("NPU_REGIONCFG") && !copied.contains("BASEP["));
        click(&mut map_cx, "region-fit");
        assert!(map.read_with(&map_cx, |m, _| m.axis_span) > 0x1_0000_0000);
    }
    #[gpui::test]
    fn mixed_kernel_selection_shows_named_parameters_and_constants(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut program = fixture();
            Arc::get_mut(&mut program).unwrap().methods[0] = mixed::method();
            let mut explorer = Explorer::empty();
            explorer.program = Some(program);
            explorer.set_method(0, cx);
            explorer
        });
        cx.simulate_resize(size(px(1300.), px(850.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let hit = view.read_with(cx, |v, _| {
            let n = v
                .graph
                .nodes
                .iter()
                .find(|n| n.selection == Selection::Operator(4))
                .unwrap();
            assert_eq!(n.properties[0].0, "weight");
            v.viewport.get().origin
                + point(
                    px(v.camera.x + (n.x + n.w / 2.) * v.camera.zoom),
                    px(v.camera.y + (n.y + n.h / 2.) * v.camera.zoom),
                )
        });
        cx.simulate_click(hit, Modifiers::none());
        cx.run_until_parked();
        let panel = view.read_with(cx, |v, _| {
            assert_eq!(v.selection, Some(Selection::Operator(4)));
            v.detail_panel.clone().unwrap()
        });
        panel.read_with(cx, |p, _| {
            assert_eq!(p.title, "aten::convolution.out");
            let parameters = p.sections.iter().find(|s| s.title == "Parameters").unwrap();
            assert!(
                parameters
                    .fields
                    .iter()
                    .any(|(k, v)| k == "stride · int[]" && v == "[2, 2]")
            );
            for name in ["padding · int[]", "output_padding · int[]"] {
                assert!(
                    parameters
                        .fields
                        .iter()
                        .any(|(k, v)| k == name && v == "[0, 0]"),
                    "Parameters sharing a value index must retain their separate names"
                );
            }
            assert!(
                p.sections
                    .iter()
                    .any(|s| s.title == "Constant · weight · v1" && s.collapsed)
            );
            assert_eq!(
                p.sections
                    .iter()
                    .filter(|s| s.title.starts_with("Output ·"))
                    .count(),
                1
            );
            assert!(!p.sections.iter().any(|s| s.title.contains("unavailable")));
        });
    }
    #[test]
    fn initial_windows_fit_small_scaled_and_offset_work_areas() {
        for (x, y, width, height) in [
            (0., 0., 1366., 728.),
            (0., 0., 960., 516.),    // 1920 x 1080 at 200%, with a taskbar
            (0., 0., 640., 456.),    // smaller display at 200%
            (64., 48., 1280., 672.), // taskbars on the top or left
            (-1280., -720., 1280., 672.),
            (0., 0., 2560., 1400.),
        ] {
            let area = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
            for preferred in [size(px(1500.), px(940.)), size(px(1450.), px(900.))] {
                let bounds = fit_window_bounds(area, preferred);
                assert!(bounds.left() >= area.left() + px(32.));
                assert!(bounds.top() >= area.top() + px(32.));
                assert!(bounds.right() <= area.right() - px(32.));
                assert!(bounds.bottom() <= area.bottom() - px(32.));
                assert!(bounds.size.width <= preferred.width);
                assert!(bounds.size.height <= preferred.height);
            }
        }
    }
    fn fixture() -> Arc<Program> {
        Arc::new(Program {
            path: "interaction fixture".into(),
            file_size: 0,
            version: 0,
            segment_base: 0,
            segments: vec![],
            named_data: Default::default(),
            methods: vec![Method {
                name: "forward".into(),
                inputs: vec![0],
                outputs: vec![1],
                values: vec![
                    Value {
                        kind: "int".into(),
                        description: "3".into(),
                        tensor: None,
                        references: vec![],
                    },
                    Value {
                        kind: "int".into(),
                        description: "4".into(),
                        tensor: None,
                        references: vec![],
                    },
                ],
                operators: vec!["example".into()],
                instructions: vec![Instruction {
                    chain: 0,
                    index: 0,
                    name: "example".into(),
                    args: vec![0, 1],
                    delegate: None,
                    kind: 1,
                    target: None,
                }],
                delegates: vec![],
                memory_sizes: vec![],
                input_structure: String::new(),
                output_structure: String::new(),
            }],
        })
    }
    fn delegate_fixture() -> Arc<Program> {
        delegate_fixture_with_commands(&[0x0105 | (1 << 16), 2, 0])
    }
    fn delegate_fixture_with_commands(commands: &[u32]) -> Arc<Program> {
        let mut xs = vec![
            u32::from_le_bytes(*b"COP1"),
            1,
            0x20000008,
            0x20000000,
            5,
            5,
            5,
            ((commands.len() as u32) << 16) | 2,
        ];
        xs.extend_from_slice(commands);
        let payload: Vec<u8> = xs.into_iter().flat_map(u32::to_le_bytes).collect();
        let mut data = vec![];
        for (name, body) in [
            ("vela_bin_stream", &[][..]),
            ("cmd_data", payload.as_slice()),
            ("vela_end_stream", &[][..]),
        ] {
            let start = data.len();
            data.resize(start + 32, 0);
            data[start..start + name.len()].copy_from_slice(name.as_bytes());
            data[start + 16..start + 20].copy_from_slice(&(body.len() as u32).to_le_bytes());
            data.extend(body);
            while data.len() % 16 != 0 {
                data.push(0);
            }
        }
        let vela = pte_explorer::vela::Vela::parse(&data, |_| unreachable!()).unwrap();
        let mut p = fixture();
        let m = &mut Arc::get_mut(&mut p).unwrap().methods[0];
        m.instructions[0].delegate = Some(0);
        m.instructions[0].name = "EthosUBackend".into();
        m.delegates.push(pte_explorer::pte::Delegate {
            id: "EthosUBackend".into(),
            location: "test".into(),
            size: data.len(),
            specs: vec![],
            vela: Some(vela),
            warning: None,
        });
        p
    }
    fn click(cx: &mut VisualTestContext, id: &'static str) {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing control {id}"));
        cx.simulate_click(bounds.center(), Modifiers::none());
    }
    fn double_click(cx: &mut VisualTestContext, position: Point<Pixels>) {
        let before = cx.windows().len();
        cx.simulate_click(position, Modifiers::none());
        assert_eq!(
            cx.windows().len(),
            before,
            "A single click must not open a window"
        );
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        assert_eq!(
            cx.windows().len(),
            before + 1,
            "A double click must open exactly one window"
        );
    }
    #[gpui::test]
    fn double_click_opens_operator_and_value_details(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut v = Explorer::empty();
            v.program = Some(fixture());
            v.set_method(0, cx);
            v
        });
        cx.simulate_resize(size(px(1300.), px(850.)));
        cx.run_until_parked();
        // Paint the camera transform computed by the initial layout before hit testing.
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        for (selection, title) in [
            (Selection::Operator(0), "example"),
            (Selection::Value(0), "Value v0"),
        ] {
            let hit = view.read_with(cx, |v, _| {
                let n = v
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.selection == selection)
                    .unwrap();
                v.viewport.get().origin
                    + point(
                        px(v.camera.x + (n.x + n.w / 2.) * v.camera.zoom),
                        px(v.camera.y + (n.y + n.h / 2.) * v.camera.zoom),
                    )
            });
            double_click(cx, hit);
            let panels = cx
                .windows()
                .into_iter()
                .filter_map(|w| w.downcast::<DetailPanel>())
                .map(|w| w.root(cx).unwrap())
                .collect::<Vec<_>>();
            let panel = panels
                .into_iter()
                .find(|p| p.read_with(cx, |p, _| p.title == title))
                .expect("Detached structured details");
            assert!(!panel.read_with(cx, |p, _| p.sections.is_empty()));
        }
        let windows = cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<DetailPanel>())
            .collect::<Vec<_>>();
        view.update(cx, |v, cx| {
            v.program = None;
            v.set_method(0, cx);
        });
        assert_eq!(windows.len(), 2);
        for window in windows {
            assert!(
                !window
                    .root(cx)
                    .unwrap()
                    .read_with(cx, |p, _| p.copy_text().is_empty())
            );
        }
    }
    #[test]
    fn delegate_ports_use_vela_abi_and_outputs_are_on_the_right() {
        use pte_explorer::vela::Io;
        let mut p = delegate_fixture();
        let m = &mut Arc::get_mut(&mut p).unwrap().methods[0];
        m.inputs.clear();
        m.outputs.clear();
        let v = m.delegates[0].vela.as_mut().unwrap();
        v.inputs.push(Io {
            shape: vec![1, 4],
            element_bytes: 1,
            offset: 0,
            region: 1,
        });
        v.outputs.push(Io {
            shape: vec![1, 4],
            element_bytes: 1,
            offset: 16,
            region: 1,
        });
        assert_eq!(
            argument_roles(m, 0),
            vec![ArgumentRole::Input, ArgumentRole::Output]
        );
        assert_eq!(
            Graph::method(m).nodes.len(),
            1,
            "Internal values must stay in the details pane"
        );
        m.inputs = vec![0];
        m.outputs = vec![1];
        let graph = Graph::method(m);
        let op = graph
            .nodes
            .iter()
            .position(|n| n.selection == Selection::Operator(0))
            .unwrap();
        let input = graph
            .nodes
            .iter()
            .position(|n| n.selection == Selection::Value(0))
            .unwrap();
        let output = graph
            .nodes
            .iter()
            .position(|n| n.selection == Selection::Value(1))
            .unwrap();
        assert!(
            graph.nodes[input].x < graph.nodes[op].x && graph.nodes[output].x > graph.nodes[op].x
        );
        assert!(
            graph
                .edges
                .iter()
                .any(|e| e.from == input && e.to == op && e.directed)
        );
        assert!(
            graph
                .edges
                .iter()
                .any(|e| e.from == op && e.to == output && e.directed)
        );
    }
    #[gpui::test]
    fn method_dropdown_and_collapsible_properties(cx: &mut TestAppContext) {
        let mut p = fixture();
        let mut extra = Arc::try_unwrap(fixture()).unwrap().methods.remove(0);
        extra.name = "configuration".into();
        Arc::get_mut(&mut p).unwrap().methods.push(extra);
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut v = Explorer::empty();
            v.program = Some(p);
            v.set_method(0, cx);
            v
        });
        cx.simulate_resize(size(px(1300.), px(900.)));
        cx.run_until_parked();
        click(cx, "method-dropdown");
        assert!(cx.debug_bounds("method-menu").is_some());
        click(cx, "method-option-1");
        assert_eq!(view.read_with(cx, |v, _| v.method), 1);
        assert!(!view.read_with(cx, |v, _| v.method_menu));
        let detail = view.read_with(cx, |v, _| v.detail_panel.as_ref().unwrap().clone());
        assert!(cx.debug_bounds("field-0-0").is_some());
        let expanded_y = cx.debug_bounds("section-1").unwrap().origin.y;
        click(cx, "section-0");
        assert!(detail.read_with(cx, |v, _| v.collapsed.contains(&0)));
        let collapsed_y = cx.debug_bounds("section-1").unwrap().origin.y;
        assert!(collapsed_y < expanded_y);
        click(cx, "section-0");
        assert!(!detail.read_with(cx, |v, _| v.collapsed.contains(&0)));
        assert_eq!(cx.debug_bounds("section-1").unwrap().origin.y, expanded_y);
    }
    #[gpui::test]
    fn long_stream_scrollbar_and_explicit_filters(cx: &mut TestAppContext) {
        let mut p = delegate_fixture();
        let v = Arc::get_mut(&mut p).unwrap().methods[0].delegates[0]
            .vela
            .as_mut()
            .unwrap();
        v.instructions = (0..1000)
            .map(|i| pte_explorer::vela::Command {
                offset: i * 4,
                raw: 0,
                payload: None,
                name: if i % 10 == 0 {
                    "NPU_OP_CONV"
                } else {
                    "NPU_SET_IFM_PRECISION"
                }
                .into(),
                value: 1,
                fields: Default::default(),
            })
            .collect();
        let (view, cx) = cx.add_window_view(|_, _| StreamView::new(p, 0, 0));
        cx.simulate_resize(size(px(1400.), px(850.)));
        cx.run_until_parked();
        let rail = cx.debug_bounds("command-scrollbar").unwrap();
        cx.simulate_click(
            rail.origin + point(px(8.), rail.size.height - px(2.)),
            Modifiers::none(),
        );
        let offset = view.read_with(cx, |v, _| v.scroll.0.borrow().base_handle.offset().y);
        assert!(offset < px(-1000.), "Rail click did not scroll: {offset:?}");
        click(cx, "command-last");
        assert!(cx.debug_bounds("command-999").is_some());
        click(cx, "command-999");
        click(cx, "filter-operations");
        assert!(view.read_with(cx, |v, _| v.operations_only && v.selected.is_none()));
        assert!(cx.debug_bounds("command-0").is_some());
        click(cx, "command-10");
        assert_eq!(view.read_with(cx, |v, _| v.selected), Some(10));
        click(cx, "command-last");
        assert!(cx.debug_bounds("command-990").is_some());
        click(cx, "filter-all");
        assert!(!view.read_with(cx, |v, _| v.operations_only));
        click(cx, "command-first");
        assert!(cx.debug_bounds("command-1").is_some());
        click(cx, "command-1");
        assert_eq!(view.read_with(cx, |v, _| v.selected), Some(1));
        let rail = cx.debug_bounds("command-scrollbar").unwrap();
        let start = rail.origin + point(px(8.), px(13.));
        let end = rail.origin + point(px(8.), rail.size.height - px(13.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        assert!(view.read_with(cx, |v, _| v.scroll.0.borrow().base_handle.offset().y) < px(-1000.));
    }
    #[gpui::test]
    fn dma_address_double_click_focuses_the_correct_span(cx: &mut TestAppContext) {
        use pte_explorer::vela::MemorySpace;
        let p = delegate_fixture_with_commands(&[
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
        ]);
        let (view, cx) = cx.add_window_view(|_, _| StreamView::new(p, 0, 0));
        cx.simulate_resize(size(px(1500.), px(900.)));
        cx.run_until_parked();
        click(cx, "filter-operations");
        click(cx, "command-5");
        let detail = view.read_with(cx, |v, _| v.detail.as_ref().unwrap().clone());
        assert!(
            detail.read_with(cx, |p, _| p.sections[0].title == "Selected instruction"
                && !p.collapsed.contains(&0)
                && p.collapsed.contains(&4)
                && p.collapsed.contains(&5)
                && p.collapsed.contains(&6))
        );
        for (selector, role, expected_region, offset) in [
            ("field-2-1", "DMA source", 0, 4294967424_u64),
            ("field-3-1", "DMA destination", 1, 512),
        ] {
            let field = cx.debug_bounds(selector).unwrap();
            let pane = cx.debug_bounds("stream-details-pane").unwrap();
            let delta = field.top() - pane.top() - px(90.);
            detail.update(cx, |p, cx| {
                let old = p.scroll.offset();
                p.scroll.set_offset(point(px(0.), old.y - delta));
                cx.notify();
            });
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let position = cx.debug_bounds(selector).unwrap().center();
            double_click(cx, position);
            let window = cx
                .windows()
                .into_iter()
                .filter_map(|w| w.downcast::<MemoryWindow>())
                .collect::<Vec<_>>()
                .into_iter()
                .find(|w| {
                    let root = w.root(cx).unwrap();
                    root.read_with(cx, |w, cx| {
                        w.map.read(cx).ranges[w.map.read(cx).selected.unwrap()].label == role
                    })
                })
                .unwrap();
            let root = window.root(cx).unwrap();
            let map = root.read_with(cx, |w, _| w.map.clone());
            let mut map_cx = VisualTestContext::from_window(window.into(), cx);
            map_cx.run_until_parked();
            map_cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let selected = map.read_with(&map_cx, |m, _| {
                let r = &m.ranges[m.selected.unwrap()];
                assert_eq!(r.space(), MemorySpace::Region(expected_region));
                assert_eq!((r.offset, r.length, r.command), (offset, Some(64), Some(5)));
                assert!(m.axis_start <= offset && offset - m.axis_start <= m.axis_span);
                assert!(
                    m.axis_span <= 128,
                    "Fit the transfer rather than the whole address space"
                );
                m.selected.unwrap()
            });
            let selector: &'static str =
                Box::leak(format!("memory-range-{selected}").into_boxed_str());
            let bar = map_cx.debug_bounds(selector).unwrap();
            let canvas = map_cx.debug_bounds("region-canvas").unwrap();
            assert!(bar.size.width > canvas.size.width / 3.);
            assert!(bar.left() >= canvas.left() && bar.right() <= canvas.right());
            assert!((bar.center().y - canvas.center().y).abs() < px(2.));
            let label: &'static str =
                Box::leak(format!("memory-label-{selected}").into_boxed_str());
            assert!(
                map_cx.debug_bounds(label).unwrap().right() <= bar.left(),
                "The row label must not obscure the selected span"
            );
            click(&mut map_cx, "region-fit");
            assert!(map.read_with(&map_cx, |m, _| m.axis_span) > 4294967424);
            click(&mut map_cx, "region-focus");
            assert!(map.read_with(&map_cx, |m, _| m.axis_span) <= 128);
            click(&mut map_cx, "copy-details");
            assert!(
                map_cx
                    .read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .contains(role)
            );
        }
        let colors = (0..8)
            .map(|n| panels::memory_color(MemorySpace::Region(n)))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(colors.len(), 8);
        assert!(!colors.contains(&panels::memory_color(MemorySpace::Internal)));
        click(cx, "copy-details");
        assert!(
            cx.read_from_clipboard()
                .unwrap()
                .text()
                .unwrap()
                .contains("NPU_REGIONCFG_0")
        );
        // A missing length stays an address marker, including in internal NPU memory.
        let mut marker = detail.read_with(cx, |p, _| {
            p.sections[2].memory_links.values().next().unwrap().clone()
        });
        marker.internal = true;
        marker.length = None;
        let window = cx.update(|_, cx| {
            cx.open_window(
                window_options("Internal address marker".into(), 1250., 850., cx),
                |_, cx| cx.new(|cx| MemoryWindow::new(Arc::new(vec![]), marker, cx)),
            )
            .unwrap()
        });
        let root = window.root(cx).unwrap();
        let map = root.read_with(cx, |w, _| w.map.clone());
        let mut marker_cx = VisualTestContext::from_window(window.into(), cx);
        marker_cx.run_until_parked();
        marker_cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(map.read_with(&marker_cx, |m, _| m.ranges[0].internal
            && m.ranges[0].length.is_none()));
        let canvas = marker_cx.debug_bounds("region-canvas").unwrap();
        let bar = marker_cx.debug_bounds("memory-range-0").unwrap();
        assert!(bar.size.width <= px(6.));
        assert!((bar.center().x - canvas.center().x - px(65.)).abs() <= px(4.));
    }
    #[gpui::test]
    fn region_map_selection_pan_zoom_and_details(cx: &mut TestAppContext) {
        use pte_explorer::vela::Io;
        let mut p = delegate_fixture();
        let v = Arc::get_mut(&mut p).unwrap().methods[0].delegates[0]
            .vela
            .as_mut()
            .unwrap();
        v.inputs.push(Io {
            shape: vec![1, 1024],
            element_bytes: 1,
            offset: 0,
            region: 1,
        });
        v.outputs.push(Io {
            shape: vec![1, 512],
            element_bytes: 1,
            offset: 4096,
            region: 1,
        });
        let (view, cx) = cx.add_window_view(|_, _| StreamView::new(p, 0, 0));
        cx.simulate_resize(size(px(1500.), px(900.)));
        cx.run_until_parked();
        click(cx, "memory-tab");
        cx.run_until_parked();
        let map = view.read_with(cx, |v, _| v.region_map.as_ref().unwrap().clone());
        assert!(cx.debug_bounds("region-canvas").is_some());
        click(cx, "memory-range-0");
        assert_eq!(map.read_with(cx, |v, _| v.selected), Some(0));
        click(cx, "copy-details");
        let copied = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(
            copied.contains("BASEP[1]") && copied.contains("1024 bytes"),
            "{copied}"
        );
        let before = map.read_with(cx, |v, _| v.camera.zoom);
        click(cx, "region-zoom-in");
        assert!(map.read_with(cx, |v, _| v.camera.zoom) > before);
        click(cx, "region-actual");
        assert_eq!(map.read_with(cx, |v, _| v.camera.zoom), 1.);
        let b = cx.debug_bounds("region-canvas").unwrap();
        let start = b.origin + point(px(30.), b.size.height - px(30.));
        let old = map.read_with(cx, |v, _| v.camera);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            start + point(px(24.), px(12.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            start + point(px(24.), px(12.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert_eq!(map.read_with(cx, |v, _| v.camera.x), old.x + 24.);
        click(cx, "region-fit");
        click(cx, "regions");
        assert!(view.read_with(cx, |v, _| !v.memory_view && v.selected.is_none()));
    }
    #[gpui::test]
    fn canvas_drag_zoom_fit_and_selection(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut v = Explorer::empty();
            v.program = Some(fixture());
            v.set_method(0, cx);
            v
        });
        cx.simulate_resize(size(px(1600.), px(950.)));
        cx.run_until_parked();
        let viewport = view.read_with(cx, |v, _| v.viewport.get());
        assert!(viewport.size.width > px(400.));
        assert!(
            view.read_with(cx, |v, _| v.graph.width * v.camera.zoom)
                <= f32::from(viewport.size.width) - 39.,
            "Initial fit: {:?}",
            view.read_with(cx, |v, _| (
                v.camera,
                v.fit_pending,
                v.viewport.get(),
                v.graph.width
            ))
        );
        let start = viewport.origin + point(px(50.), px(300.));
        let before = view.read_with(cx, |v, _| v.camera);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            start + point(px(40.), px(25.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            start + point(px(40.), px(25.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        let after = view.read_with(cx, |v, _| v.camera);
        assert_eq!(cx.windows().len(), 1, "Dragging must not open details");
        assert_eq!(after.x, before.x + 40.);
        assert_eq!(after.y, before.y + 25.);
        click(cx, "zoom-in");
        assert!(view.read_with(cx, |v, _| v.camera.zoom) > before.zoom);
        click(cx, "actual-size");
        assert_eq!(view.read_with(cx, |v, _| v.camera.zoom), 1.);
        click(cx, "fit");
        let (viewport, camera, graph) =
            view.read_with(cx, |v, _| (v.viewport.get(), v.camera, v.graph.clone()));
        assert!(graph.width * camera.zoom <= f32::from(viewport.size.width) + 0.01);
        let n = graph
            .nodes
            .iter()
            .find(|n| n.selection == Selection::Value(0))
            .unwrap();
        let hit = viewport.origin
            + point(
                px(camera.x + (n.x + n.w / 2.) * camera.zoom),
                px(camera.y + (n.y + n.h / 2.) * camera.zoom),
            );
        cx.simulate_click(hit, Modifiers::none());
        assert_eq!(
            view.read_with(cx, |v, _| v.selection),
            Some(Selection::Value(0))
        );
        click(cx, "metadata");
        assert!(view.read_with(cx, |v, _| v.selection.is_none()));
        let anchor = viewport.center();
        let old = view.read_with(cx, |v, _| v.camera.zoom);
        cx.simulate_event(ScrollWheelEvent {
            position: anchor,
            delta: ScrollDelta::Lines(point(0., 2.)),
            ..Default::default()
        });
        assert!(view.read_with(cx, |v, _| v.camera.zoom) > old);
    }
    #[gpui::test]
    fn delegate_details_remain_visible_when_resized(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut v = Explorer::empty();
            v.program = Some(delegate_fixture());
            v.set_method(0, cx);
            v.select(Selection::Operator(0), cx);
            v
        });
        for (width, height) in [
            (700., 500.),
            (700., 800.),
            (900., 800.),
            (1200., 800.),
            (1600., 800.),
        ] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            for selector in [
                "right-pane",
                "stream-details-pane",
                "regions",
                "copy-details",
                "fit",
                "actual-size",
            ] {
                let bounds = cx
                    .debug_bounds(selector)
                    .unwrap_or_else(|| panic!("missing {selector} at {width}px"));
                assert!(
                    bounds.origin.x >= px(0.)
                        && bounds.origin.x + bounds.size.width <= px(width + 1.),
                    "{selector} outside {width}px window: {bounds:?}"
                );
                assert!(
                    bounds.origin.y >= px(0.)
                        && bounds.origin.y + bounds.size.height <= px(height + 1.),
                    "{selector} outside window vertically: {bounds:?}"
                );
            }
            let viewport = view.read_with(cx, |v, _| v.viewport.get());
            assert!(viewport.size.width >= px(150.));
            click(cx, "fit");
            click(cx, "actual-size");
            assert_eq!(view.read_with(cx, |v, _| v.camera.zoom), 1.);
        }
    }
    #[gpui::test]
    fn stream_selection_and_detached_window_are_independent(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut view = Explorer::empty();
            view.program = Some(delegate_fixture());
            view.set_method(0, cx);
            view
        });
        cx.simulate_resize(size(px(1500.), px(900.)));
        cx.run_until_parked();
        // Paint the initial auto-fit before locating the node.
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let hit = view.read_with(cx, |v, _| {
            let n = v
                .graph
                .nodes
                .iter()
                .find(|n| n.selection == Selection::Operator(0))
                .unwrap();
            v.viewport.get().origin
                + point(
                    px(v.camera.x + (n.x + n.w - 8.) * v.camera.zoom),
                    px(v.camera.y + (n.y + n.h / 2.) * v.camera.zoom),
                )
        });
        double_click(cx, hit);
        let embedded = view.read_with(cx, |v, _| v.stream.as_ref().unwrap().clone());
        click(cx, "command-1");
        assert_eq!(embedded.read_with(cx, |v, _| v.selected), Some(1));
        click(cx, "copy-details");
        assert!(cx.read_from_clipboard().is_some());
        let windows = cx.windows();
        assert_eq!(windows.len(), 2);
        let other_window = windows
            .into_iter()
            .find(|h| h.downcast::<StreamView>().is_some())
            .unwrap();
        let detached = other_window.downcast::<StreamView>().unwrap();
        let other = detached.root(cx).unwrap();
        assert_eq!(other.read_with(cx, |v, _| v.selected), None);
        let mut other_cx = VisualTestContext::from_window(other_window, cx);
        other_cx.run_until_parked();
        click(&mut other_cx, "command-0");
        assert_eq!(other.read_with(&other_cx, |v, _| v.selected), Some(0));
        assert_eq!(embedded.read_with(&other_cx, |v, _| v.selected), Some(1));
        view.update(&mut other_cx, |v, cx| {
            v.program = Some(fixture());
            v.set_method(0, cx);
        });
        click(&mut other_cx, "copy-details");
        assert!(
            other_cx
                .read_from_clipboard()
                .unwrap()
                .text()
                .unwrap()
                .contains("NPU_SET_IFM_PRECISION")
        );
    }
}
