use dioxus::prelude::*;
use crate::types::ScanResults;

#[derive(Props, PartialEq, Clone)]
pub struct ProgressOverlayProps {
    pub progress: Signal<Option<(usize,usize)>>,
    pub scanning: Signal<bool>,
    pub recursive_current: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub results: Signal<ScanResults>,
    // expansion control
    pub show_expanded: Signal<bool>,
    // action callbacks (provided by parent) kept simple for now
    pub on_select_all: EventHandler<()>,
    pub on_filter_images: EventHandler<()>,
    pub on_filter_videos: EventHandler<()>,
    pub on_filter_all: EventHandler<()>,
    pub on_sort_name: EventHandler<()>,
    pub on_sort_date: EventHandler<()>,
    pub on_sort_size: EventHandler<()>,
    // initial position optional in future
}

#[allow(non_snake_case)]
pub fn ProgressOverlay(props: ProgressOverlayProps) -> Element {
    let ProgressOverlayProps { progress, scanning, recursive_current, scan_started, scan_finished, results, mut show_expanded, on_select_all, on_filter_images, on_filter_videos, on_filter_all, on_sort_name, on_sort_date, on_sort_size } = props;

    // Local signals for position & drag state
    let mut pos = use_signal(|| (None::<i32>, None::<i32>)); // (left, top) if None -> use default right/bottom
    let mut dragging = use_signal(|| None::<(i32,i32,(i32,i32))>); // (start_mouse_x, start_mouse_y, (orig_left,orig_top))

    let prog = progress.read().clone();
    let started_opt = scan_started.read().clone();
    let finished_opt = scan_finished.read().clone();
    let elapsed = match (started_opt, finished_opt) { (Some(st), Some(fin)) => fin.duration_since(st), (Some(st), None) => st.elapsed(), _ => std::time::Duration::default() };
    let secs = elapsed.as_secs_f32();
    let done = !*scanning.read();
    let found_count = results.read().items.len();
    let expanded = *show_expanded.read();
    let container_classes = if expanded { "progress-overlay-btn bg-muted overflow-hidden flex flex-col border shadow-xl" } else { "bg-muted overflow-hidden flex flex-col border" };
    // Compute style based on drag position
    let style_pos = if let (Some(l), Some(t)) = *pos.read() {
        format!("position:absolute; left:{}px; top:{}px; border-radius:10px; min-width:240px; max-width:46%; border-color: var(--error); z-index:120;", l, t)
    } else {
        // default anchored bottom-right
        "position:absolute; right: 2%; bottom: 12px; border-radius:10px; min-width:240px; max-width:46%; border-color: var(--error); z-index:120;".to_string()
    };
    rsx! { div { class: "{container_classes}", style: "{style_pos}",
        // progress bar region
        if let Some((scanned,total)) = prog { if total > 0 { { let pct = (scanned as f32 / total.max(1) as f32 * 100.0).min(100.0); rsx!{ div { class: "h-1", class: if done { "bg-green-500" } else { "bg-accent" }, style: "width:{pct}%; transition:width .12s linear;" } } } } else { div { class: "h-1 bg-accent animate-pulse", style: "width:40%; position:absolute; left:0; animation: scan-indeterminate 1.2s linear infinite;" } } } else { div { class: "h-1 bg-accent animate-pulse", style: "width:30%;" } }
        div { class: "flex flex-wrap gap-3 px-2 py-1 text-11px text-weak items-center", style: "user-select:none;",
            // Drag handle button
            button { class: "mini-btn cursor-move p-1 rounded hover:bg-accent/10", title: "Drag",
                onmousedown: move |evt| {
                    if evt.trigger_button().is_some() {
                        let client = evt.client_coordinates();
                        let ex = client.x.round() as i32;
                        let ey = client.y.round() as i32;
                        let (orig_l, orig_t) = if let (Some(l), Some(t)) = *pos.read() { (l,t) } else { (ex, ey) };
                        dragging.set(Some((ex, ey, (orig_l, orig_t))));
                        if pos.read().0.is_none() { pos.set((Some(orig_l), Some(orig_t))); }
                    }
                },
                onmousemove: move |evt| {
                    if let Some((sx, sy, (ol, ot))) = *dragging.read() {
                        let client = evt.client_coordinates();
                        let cx = client.x.round() as i32;
                        let cy = client.y.round() as i32;
                        let dx = cx - sx;
                        let dy = cy - sy;
                        if dx.abs() + dy.abs() > 1 { pos.set((Some((ol + dx).max(8)), Some((ot + dy).max(8)))); }
                    }
                },
                onmouseup: move |_| { dragging.set(None); },
                onmouseleave: move |_| { dragging.set(None); },
                i { class: "material-icons text-sm opacity-70", "open_with" }
            }
            span { class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium ", class: if done { "scanning-done" } else { "scanning" }, { if done { "Done" } else if *recursive_current.read() { "Deep" } else { "Shallow" } } }
            if let Some((scanned,total)) = prog {
                if total > 0 { {{ let pct = scanned as f32 * 100.0 / total.max(1) as f32; let pct_rounded = pct.round() as i32; let rate = if secs>0.15 { scanned as f32 / secs } else { 0.0 }; rsx! { span { "{scanned} / {total} ({pct_rounded}%)" } span { "found {found_count}" } if done { span { "in {secs:.1}s" } } if !done && rate > 0.1 { span { "{rate:.1} items/s" } } } }} } else { {{ let rate = if secs>0.15 { scanned as f32 / secs } else { 0.0 }; rsx! { span { "{scanned} items" } span { "found {found_count}" } if !done && rate > 0.1 { span { "{rate:.1} items/s" } } { let txt = if done { format!("in {:.1}s", secs) } else { format!("elapsed {:.1}s", secs) }; rsx!{ span { "{txt}" } } } } }} }
            } else { span { if done { "No items" } else { "Starting scan..." } } }
            button { class: "mini-btn ml-auto rounded-full transition p-1", title: if expanded { "Collapse" } else { "Expand" }, onclick: move |_| { let cur = *show_expanded.read(); show_expanded.set(!cur); }, i { class: "material-icons mini-btn text-sm", { if expanded { "keyboard_arrow_down" } else { "keyboard_arrow_up" } } } }
        }
        if expanded {
            div { class: "px-2 pb-2 flex flex-col gap-2 border-t border-stroke bg-panel/60 backdrop-blur-sm",
                // actions row 1
                div { class: "flex gap-2 flex-wrap", button { class: "mini-btn", onclick: move |_| on_select_all.call(()), "Select All" } button { class: "mini-btn", onclick: move |_| on_filter_images.call(()), "Images" } button { class: "mini-btn", onclick: move |_| on_filter_videos.call(()), "Videos" } button { class: "mini-btn", onclick: move |_| on_filter_all.call(()), "All" } }
                // sorting
                div { class: "flex gap-2 flex-wrap", span { class: "text-8px uppercase tracking-wide text-weak", "Sort:" } button { class: "mini-btn", onclick: move |_| on_sort_name.call(()), "Name" } button { class: "mini-btn", onclick: move |_| on_sort_date.call(()), "Date" } button { class: "mini-btn", onclick: move |_| on_sort_size.call(()), "Size" } }
                // placeholder for columns config
                div { class: "flex gap-2 flex-wrap text-8px text-weak", span { "Columns config coming soon..." } }
            }
        }
    } }
}

// simple utility styling classes may be defined in global css: .mini-btn
