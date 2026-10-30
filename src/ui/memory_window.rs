use super::*;
use pte_explorer::vela::MemoryRange;

pub(super) struct MemoryWindow {
    pub map: Entity<RegionMap>,
}
impl MemoryWindow {
    pub fn overview(ranges: Arc<Vec<MemoryRange>>, cx: &mut Context<Self>) -> Self {
        let map = cx.new(|cx| RegionMap::arenas(ranges, cx));
        cx.observe(&map, |_, _, cx| cx.notify()).detach();
        Self { map }
    }
    pub fn new(ranges: Arc<Vec<MemoryRange>>, target: MemoryRange, cx: &mut Context<Self>) -> Self {
        let map = cx.new(|cx| RegionMap::focused(ranges, target, cx));
        cx.observe(&map, |_, _, cx| cx.notify()).detach();
        Self { map }
    }
}
impl Render for MemoryWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let detail = self.map.read(cx).detail.clone();
        let copy = detail.read(cx).copy_text();
        div()
            .size_full()
            .flex()
            .font_family(system_font())
            .text_size(px(14.))
            .text_color(rgb(TEXT))
            .bg(rgb(BG))
            .child(div().flex_1().min_w_0().h_full().child(self.map.clone()))
            .child(
                div()
                    .w(relative(0.36))
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(rgb(BORDER))
                    .child(
                        button("copy-details", "Copy details").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                        }),
                    )
                    .child(div().flex_1().min_h_0().child(detail)),
            )
    }
}
