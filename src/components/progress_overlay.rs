use dioxus::{desktop::DesktopContext, prelude::*};
use crate::types::ScanResults;

#[derive(Props, PartialEq, Clone)]
pub struct ProgressOverlayProps {
    pub progress: Signal<Option<(usize,usize)>>,
    pub scanning: Signal<bool>,
    pub recursive_current: Signal<bool>,
    pub scan_started: Signal<Option<std::time::Instant>>,
    pub scan_finished: Signal<Option<std::time::Instant>>,
    pub results: Signal<ScanResults>,
    // bulk generation (AI description) progress (done,total)
    pub bulk_progress: Signal<(usize,usize)>,
    pub bulk_generating: Signal<bool>,
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
    let ProgressOverlayProps { progress, scanning, recursive_current, scan_started, scan_finished, results, bulk_progress, bulk_generating, mut show_expanded, on_select_all, on_filter_images, on_filter_videos, on_filter_all, on_sort_name, on_sort_date, on_sort_size } = props;

    // Local signals for position & drag state
    let mut pos = use_signal(|| (None::<i32>, None::<i32>)); // (left, top) if None -> default anchored bottom-right
    let mut dragging = use_signal(|| None::<(i32,i32,(i32,i32))>); // (start_mouse_x, start_mouse_y, (orig_left,orig_top))
    let mut locked_width = use_signal(|| None::<i32>); // snapshot width after first drag
    let mut mounted_node = use_signal(|| None::<std::rc::Rc<MountedData>>);

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
        let w_part = if let Some(w) = *locked_width.read() { format!("width:{}px;", w) } else { String::new() };
        format!("position:absolute; left:{}px; top:{}px; {} border-radius:10px; min-width:240px; border-color: var(--error); z-index:120;", l, t, w_part)
    } else {
        "position:absolute; right: 2%; bottom: 12px; border-radius:10px; min-width:240px; border-color: var(--error); z-index:120;".to_string()
    };
    rsx! { div { class: "{container_classes}", style: "{style_pos}",
        onmounted: move |cx| {
            mounted_node.set(Some(cx.data()));
            // attempt to read width asynchronously
            let mut locked_width_sig = locked_width.clone();
            let node = cx.data();
            spawn(async move {
                if locked_width_sig.read().is_none() {
                    if let Ok(rect) = node.get_client_rect().await { locked_width_sig.set(Some(rect.size.width as i32)); }
                }
            });
        },
        // global-ish mousemove while dragging (bubble from document root) to allow moving outside header
        onmousemove: move |evt| {
            if let Some((sx, sy, (ol, ot))) = *dragging.read() {
                let c = evt.client_coordinates();
                let cx = c.x.round() as i32; let cy = c.y.round() as i32;
                let dx = cx - sx; let dy = cy - sy;
                if dx != 0 || dy != 0 { pos.set((Some((ol + dx).max(4)), Some((ot + dy).max(4)))); }
                consume_context::<DesktopContext>().request_redraw();
            }
        },
        onmouseup: move |_| { if dragging.read().is_some() { dragging.set(None); } },
        // progress bar region (using built-in progress element)
        div { class: "w-full h-1 relative overflow-hidden bg-muted/60", style: "border-top-left-radius:10px;border-top-right-radius:10px;",
            {
                match prog {
                    Some((scanned,total)) if total > 0 => {
                        let val_current = scanned.min(total);
                        rsx!{ progress { value: val_current, max: total, class: "w-full h-1 appearance-none [&::-webkit-progress-bar]:bg-transparent [&::-webkit-progress-value]:bg-accent [&::-moz-progress-bar]:bg-accent transition-[width] duration-150" } }
                    }
                    Some((_scanned,_total)) => rsx!{ progress { class: "w-full h-1 indeterminate-progress" } },
                    None => rsx!{ progress { class: "w-full h-1 indeterminate-progress" } },
                }
            }
            if done { div { class: "absolute inset-0 bg-green-500/80 mix-blend-multiply pointer-events-none", style: "opacity:0.35;" } }
        }
        div { class: "flex flex-wrap gap-3 px-2 py-1 text-11px text-weak items-center cursor-move", style: "user-select:none;",
            onmousedown: move |evt| {
                if evt.trigger_button().is_some() {
                    let client = evt.client_coordinates();
                    let el = evt.element_coordinates(); // position inside header row
                    let ex = client.x.round() as i32;
                    let ey = client.y.round() as i32;
                    let offset_x = el.x.round() as i32;
                    let offset_y = el.y.round() as i32;
                    // preserve offset so window doesn't 'resize' or jump
                    let (orig_l, orig_t) = if let (Some(l), Some(t)) = *pos.read() { (l,t) } else { (ex - offset_x, ey - offset_y) };
                    dragging.set(Some((ex, ey, (orig_l, orig_t))));
                    if pos.read().0.is_none() { pos.set((Some(orig_l), Some(orig_t))); }
                    if locked_width.read().is_none() {
                        // heuristic: approximate width via 340px if not measured
                        locked_width.set(Some(340));
                    }
                    consume_context::<DesktopContext>().request_redraw();
                }
            },
            onmouseleave: move |_| { if dragging.read().is_some() { /* keep dragging active; global onmousemove handles */ } },
            i { class: "material-icons text-sm opacity-70", "open_with" }
            span { class: "px-1.5 py-0.5 rounded-full text-10px tracking-wide uppercase font-medium ", class: if done { "scanning-done" } else { "scanning" }, { if done { "Done" } else if *recursive_current.read() { "Deep" } else { "Shallow" } } }
            if let Some((scanned,total)) = prog {
                if total > 0 { {{ let pct = scanned as f32 * 100.0 / total.max(1) as f32; let pct_rounded = pct.round() as i32; let rate = if secs>0.15 { scanned as f32 / secs } else { 0.0 }; rsx! { span { "{scanned} / {total} ({pct_rounded}%)" } span { "found {found_count}" } if done { span { "in {secs:.1}s" } } if !done && rate > 0.1 { span { "{rate:.1} items/s" } } } }} } else { {{ let rate = if secs>0.15 { scanned as f32 / secs } else { 0.0 }; rsx! { span { "{scanned} items" } span { "found {found_count}" } if !done && rate > 0.1 { span { "{rate:.1} items/s" } } { let txt = if done { format!("in {:.1}s", secs) } else { format!("elapsed {:.1}s", secs) }; rsx!{ span { "{txt}" } } } } }} }
            } else { span { if done { "No items" } else { "Starting scan..." } } }
            // bulk generation inline status (only if active or some progress)
            { let (bd,bt) = *bulk_progress.read(); if *bulk_generating.read() || bt > 0 { let pct = if bt>0 { (bd as f32 * 100.0 / bt as f32).round() as i32 } else { 0 }; rsx!{ span { class: "ml-1 px-1 py-0.5 rounded-full bg-accent/20 text-accent text-9px", { if bt>0 { format!("Desc {bd}/{bt} ({pct}%)") } else { "Preparing...".into() } } } } } else { rsx!{} } }
            button { class: "btn ml-auto rounded-full transition p-1", title: if expanded { "Collapse" } else { "Expand" }, onclick: move |_| { let cur = *show_expanded.read(); show_expanded.set(!cur); }, i { class: "material-icons btn text-sm", { if expanded { "keyboard_arrow_down" } else { "keyboard_arrow_up" } } } }
        }
        if expanded {
            div { class: "px-2 pb-2 flex flex-col gap-2 border-t border-stroke bg-panel/60 backdrop-blur-sm",
                // actions row 1
                div { class: "flex gap-2 flex-wrap", button { class: "btn", onclick: move |_| on_select_all.call(()), "Select All" } button { class: "btn", onclick: move |_| on_filter_images.call(()), "Images" } button { class: "btn", onclick: move |_| on_filter_videos.call(()), "Videos" } button { class: "btn", onclick: move |_| on_filter_all.call(()), "All" } }
                // Bulk generation progress detail (if any)
                { let (bd,bt) = *bulk_progress.read(); if *bulk_generating.read() || bt>0 { let pct = if bt>0 { (bd as f32 * 100.0 / bt as f32).round() as i32 } else { 0 }; rsx!{ div { class: "flex items-center gap-2 text-10px text-weak",
                        span { "Descriptions: {bd} / {bt} ({pct}%)" }
                        { if *bulk_generating.read() { rsx!{ span { class: "animate-pulse text-accent", "Generating..." } } } else if bt>0 && bd>=bt { rsx!{ span { class: "text-green-500", "Done" } } } else { rsx!{} } }
                    } } } else { rsx!{} } }
                // sorting
                div { class: "flex gap-2 flex-wrap", span { class: "text-8px uppercase tracking-wide text-weak", "Sort:" } button { class: "btn", onclick: move |_| on_sort_name.call(()), "Name" } button { class: "btn", onclick: move |_| on_sort_date.call(()), "Date" } button { class: "btn", onclick: move |_| on_sort_size.call(()), "Size" } }
                // placeholder for columns config
                div { class: "flex gap-2 flex-wrap text-8px text-weak", span { "Columns config coming soon..." } }
            }
        }
    } }
}

// simple utility styling classes may be defined in global css: .btn
